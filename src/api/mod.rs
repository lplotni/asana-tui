//! Asana REST API client: auth, pagination, rate limits, error mapping.

pub mod notifications;

use std::time::Duration;

use reqwest::StatusCode;
use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::config::Token;

pub const ASANA_API_URL: &str = "https://app.asana.com/api/1.0";

/// Used when a 429 response carries no usable `Retry-After` header.
const DEFAULT_RETRY_AFTER: Duration = Duration::from_secs(1);

/// One page of a paginated Asana list response. Only `next_page == None`
/// marks the end; pages can be shorter than requested or empty.
#[derive(Debug, Deserialize)]
pub struct Page<T> {
    pub data: Vec<T>,
    pub next_page: Option<NextPage>,
}

#[derive(Debug, Deserialize)]
pub struct NextPage {
    pub offset: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("Asana rejected the token — check ASANA_TOKEN or ASANA_TOKEN_CMD")]
    Unauthorized,
    #[error("Asana rate limit hit twice in a row — try again in a minute")]
    RateLimited,
    #[error("Asana API error {status}: {message}")]
    Status { status: u16, message: String },
    #[error("Could not reach Asana: {0}")]
    Network(String),
}

impl ApiError {
    /// Process exit code: 2 for authentication problems, 1 for everything else.
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Unauthorized => 2,
            _ => 1,
        }
    }
}

#[derive(Deserialize)]
struct ErrorBody {
    errors: Vec<ErrorMessage>,
}

#[derive(Deserialize)]
struct ErrorMessage {
    message: String,
}

pub struct Client {
    http: reqwest::Client,
    base_url: String,
    token: String,
}

impl Client {
    pub fn new(token: &Token, base_url: impl Into<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url: base_url.into(),
            token: token.expose().to_owned(),
        }
    }

    /// GET `path` and decode the JSON body. A 429 is retried once after
    /// `Retry-After`; a second 429 is `RateLimited`.
    pub(crate) async fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<T, ApiError> {
        let mut retried = false;
        loop {
            let response = self
                .http
                .get(format!("{}{path}", self.base_url))
                .bearer_auth(&self.token)
                .query(query)
                .send()
                .await
                .map_err(network)?;

            match response.status() {
                status if status.is_success() => {
                    return response.json().await.map_err(network);
                }
                StatusCode::TOO_MANY_REQUESTS if !retried => {
                    retried = true;
                    tokio::time::sleep(retry_after(&response)).await;
                }
                StatusCode::TOO_MANY_REQUESTS => return Err(ApiError::RateLimited),
                StatusCode::UNAUTHORIZED => return Err(ApiError::Unauthorized),
                status => {
                    let message = response
                        .json::<ErrorBody>()
                        .await
                        .ok()
                        .and_then(|body| body.errors.into_iter().next())
                        .map(|e| e.message)
                        .unwrap_or_else(|| status.to_string());
                    return Err(ApiError::Status {
                        status: status.as_u16(),
                        message,
                    });
                }
            }
        }
    }
}

fn retry_after(response: &reqwest::Response) -> Duration {
    response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse().ok())
        .map(Duration::from_secs)
        .unwrap_or(DEFAULT_RETRY_AFTER)
}

/// Keeps only the error's description: reqwest errors include the request URL,
/// which is harmless today but must never grow to include credentials.
fn network(err: reqwest::Error) -> ApiError {
    let kind = if err.is_timeout() {
        "timed out"
    } else if err.is_decode() {
        "unexpected response format"
    } else if err.is_connect() {
        "connection failed"
    } else {
        "request failed"
    };
    ApiError::Network(kind.to_owned())
}
