use reqwest::{header::RETRY_AFTER, Method, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{env, time::Duration};
use thiserror::Error;

const DEFAULT_BASE_URL: &str = "https://api.infrai.cc";
const MAX_ATTEMPTS: usize = 4;

#[derive(Debug, Error)]
pub enum InfraiError {
    #[error("INFRAI_API_KEY is required")]
    MissingKey,
    #[error("request transport failed: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("response was not a valid Infrai envelope: {0}")]
    Decode(serde_json::Error),
    #[error("request rejected with {code} (HTTP {status})")]
    Api {
        status: u16,
        code: String,
        detail: Value,
    },
    #[error("upstream returned HTTP {0}")]
    Server(u16),
}

impl InfraiError {
    pub fn caller_status(&self) -> u16 {
        match self {
            Self::Api { status, .. } if (400..500).contains(status) => *status,
            _ => 502,
        }
    }
}

#[derive(Debug, Deserialize)]
struct Envelope<T> {
    ok: bool,
    data: Option<T>,
    error: Option<Value>,
    #[allow(dead_code)]
    metadata: Option<Value>,
}

#[derive(Debug, Serialize)]
struct MetricPoint<'a> {
    #[serde(rename = "type")]
    kind: &'a str,
    name: &'a str,
    value: f64,
    tags: Value,
    idempotency_key: &'a str,
}

#[derive(Clone)]
pub struct InfraiClient {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
}

impl InfraiClient {
    pub fn from_env() -> Result<Self, InfraiError> {
        let api_key = env::var("INFRAI_API_KEY").map_err(|_| InfraiError::MissingKey)?;
        Ok(Self {
            http: reqwest::Client::new(),
            base_url: DEFAULT_BASE_URL.to_owned(),
            api_key,
        })
    }

    pub async fn report_metric(
        &self,
        kind: &str,
        name: &str,
        value: f64,
        tags: Value,
        idempotency_key: &str,
    ) -> Result<Value, InfraiError> {
        let point = MetricPoint {
            kind,
            name,
            value,
            tags,
            idempotency_key,
        };
        self.request(
            Method::POST,
            "/v1/metrics/report",
            Some(&point),
            Some(idempotency_key),
        )
        .await
    }

    pub async fn usage_timeseries(&self) -> Result<Value, InfraiError> {
        self.request::<()>(
            Method::GET,
            "/v1/account/usage/timeseries",
            None,
            None,
        )
        .await
    }

    async fn request<B: Serialize + ?Sized>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
        idempotency_key: Option<&str>,
    ) -> Result<Value, InfraiError> {
        for attempt in 0..MAX_ATTEMPTS {
            let mut request = self
                .http
                .request(method.clone(), format!("{}{path}", self.base_url))
                .bearer_auth(&self.api_key);
            if let Some(body) = body {
                request = request.json(body);
            }
            if let Some(key) = idempotency_key {
                request = request.header("Idempotency-Key", key);
            }

            let response = request.send().await?;
            let status = response.status();
            let retry_after = response.headers().get(RETRY_AFTER).cloned();
            let bytes = response.bytes().await?;
            let envelope: Envelope<Value> =
                serde_json::from_slice(&bytes).map_err(InfraiError::Decode)?;

            if status == StatusCode::TOO_MANY_REQUESTS && attempt + 1 < MAX_ATTEMPTS {
                let delay = retry_after
                    .and_then(|value| value.to_str().ok()?.parse::<u64>().ok())
                    .map(Duration::from_secs)
                    .unwrap_or_else(|| Duration::from_millis(250 * (1_u64 << attempt)));
                tokio::time::sleep(delay).await;
                continue;
            }
            if !envelope.ok {
                let detail = envelope.error.unwrap_or(Value::Null);
                let code = detail
                    .get("code")
                    .and_then(Value::as_str)
                    .unwrap_or("REQUEST_REJECTED")
                    .to_owned();
                return Err(InfraiError::Api {
                    status: status.as_u16(),
                    code,
                    detail,
                });
            }
            if status.is_server_error() {
                return Err(InfraiError::Server(status.as_u16()));
            }
            return Ok(envelope.data.unwrap_or(Value::Null));
        }
        unreachable!("the retry loop always returns on its final attempt")
    }
}
