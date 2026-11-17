//! Asana REST API client: auth, pagination, rate limits, error mapping.

pub mod attachments;
pub mod notifications;
pub mod projects;
pub mod tasks;

use std::time::Duration;

use reqwest::StatusCode;
use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::config::Token;

pub const ASANA_API_URL: &str = "https://app.asana.com/api/1.0";

/// How long to wait for a connection to Asana.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// How long a whole request may take.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

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
        Self::with_timeouts(token, base_url, CONNECT_TIMEOUT, REQUEST_TIMEOUT)
    }

    /// A client whose requests give up after the given times, so a stalled
    /// connection (sleep, network change) can't hang a refresh forever.
    fn with_timeouts(
        token: &Token,
        base_url: impl Into<String>,
        connect: Duration,
        request: Duration,
    ) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(connect)
            .timeout(request)
            .build()
            .expect("static client configuration is valid");
        Self {
            http,
            base_url: base_url.into(),
            token: token.expose().to_owned(),
        }
    }

    /// GET `path` and decode the JSON body.
    pub(crate) async fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<T, ApiError> {
        let url = format!("{}{path}", self.base_url);
        self.send(|| self.http.get(&url).query(query)).await
    }

    /// POST `body` as JSON to `path` and decode the JSON response.
    pub(crate) async fn post<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
        body: &serde_json::Value,
    ) -> Result<T, ApiError> {
        let url = format!("{}{path}", self.base_url);
        self.send(|| self.http.post(&url).query(query).json(body))
            .await
    }

    /// PUT `body` as JSON to `path` and decode the JSON response.
    pub(crate) async fn put<T: DeserializeOwned>(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<T, ApiError> {
        let url = format!("{}{path}", self.base_url);
        self.send(|| self.http.put(&url).json(body)).await
    }

    /// Sends the request built by `request` with auth. A 429 is retried once
    /// after `Retry-After`; a second 429 is `RateLimited`.
    async fn send<T: DeserializeOwned>(
        &self,
        request: impl Fn() -> reqwest::RequestBuilder,
    ) -> Result<T, ApiError> {
        let mut retried = false;
        loop {
            let response = request()
                .bearer_auth(&self.token)
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

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn a_stalled_request_times_out_instead_of_hanging() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(5)))
            .mount(&server)
            .await;
        let client = Client::with_timeouts(
            &Token::new("t"),
            server.uri(),
            Duration::from_secs(1),
            Duration::from_millis(200),
        );

        let result: Result<serde_json::Value, ApiError> = client.get("/slow", &[]).await;

        assert!(
            matches!(&result, Err(ApiError::Network(kind)) if kind == "timed out"),
            "{result:?}"
        );
    }
}
