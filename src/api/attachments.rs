//! `GET /attachments?parent={task}`.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::{ApiError, CONNECT_TIMEOUT, Client, Page};

/// How long a whole file download may take.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(60);

/// Attachment fields shown in the task view.
pub const ATTACHMENT_FIELDS: &str = "name,resource_subtype,size,download_url";

/// A file on a task: uploaded to Asana (`resource_subtype == "asana"`) or a
/// link to another service (`gdrive`, `dropbox`, …).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Attachment {
    pub gid: String,
    pub name: String,
    pub resource_subtype: String,
    /// Bytes; only known for files uploaded to Asana.
    #[serde(default)]
    pub size: Option<u64>,
    /// A short-lived, pre-signed link to the file itself (no token needed).
    #[serde(default)]
    pub download_url: Option<String>,
}

impl Attachment {
    /// Whether the file is an image we can decode, judged by its name.
    pub fn is_image(&self) -> bool {
        let extension = self
            .name
            .rsplit_once('.')
            .map(|(_, ext)| ext.to_ascii_lowercase());
        matches!(
            extension.as_deref(),
            Some("png" | "jpg" | "jpeg" | "gif" | "webp")
        )
    }
}

impl Client {
    pub async fn attachments_page(
        &self,
        parent_gid: &str,
        offset: Option<&str>,
    ) -> Result<Page<Attachment>, ApiError> {
        let mut query = vec![
            ("parent", parent_gid.to_owned()),
            ("limit", "100".to_owned()),
            ("opt_fields", ATTACHMENT_FIELDS.to_owned()),
        ];
        if let Some(offset) = offset {
            query.push(("offset", offset.to_owned()));
        }
        self.get("/attachments", &query).await
    }
}

/// Downloads a pre-signed `download_url`. It goes to the file host (S3),
/// not Asana, so the request carries no token. Fails when the file is bigger
/// than `max_bytes`, without reading the rest.
pub async fn download(url: &str, max_bytes: usize) -> Result<Vec<u8>, ApiError> {
    let network = |e: reqwest::Error| ApiError::Network(e.to_string());
    let http = reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(DOWNLOAD_TIMEOUT)
        .build()
        .expect("static client configuration is valid");
    let mut response = http.get(url).send().await.map_err(network)?;
    let status = response.status();
    if !status.is_success() {
        return Err(ApiError::Status {
            status: status.as_u16(),
            message: "the attachment link didn't work (it may have expired)".into(),
        });
    }
    if response
        .content_length()
        .is_some_and(|len| len > max_bytes as u64)
    {
        return Err(ApiError::TooLarge(max_bytes));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(network)? {
        if bytes.len() + chunk.len() > max_bytes {
            return Err(ApiError::TooLarge(max_bytes));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(name: &str) -> Attachment {
        Attachment {
            gid: "1".into(),
            name: name.into(),
            resource_subtype: "asana".into(),
            size: None,
            download_url: None,
        }
    }

    #[test]
    fn images_are_recognized_by_extension_in_any_case() {
        assert!(named("shot.png").is_image());
        assert!(named("Photo.JPEG").is_image());
        assert!(named("a.b.webp").is_image());
        assert!(!named("Spec.pdf").is_image());
        assert!(!named("png").is_image());
    }
}
