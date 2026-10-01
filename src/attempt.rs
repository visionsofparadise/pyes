use std::time::{Duration, SystemTime};

use serde_json::Value;

use crate::answers_of::{answers_of, failure_of, Failure};

pub const BASE_URL: &str = "https://api.typesafe.ai";
pub const TIMEOUT: Duration = Duration::from_secs(30);

pub struct Client {
    agent: ureq::Agent,
    base_url: String,
    key: String,
}

impl Client {
    pub fn new(base_url: String, key: String) -> Client {
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(TIMEOUT))
            .build()
            .into();

        Client {
            agent,
            base_url,
            key,
        }
    }
}

fn transient_of(error: ureq::Error) -> Failure {
    Failure::Transient {
        message: format!("request failed: {error}"),
        retry_after: None,
    }
}

pub fn attempt(client: &Client, body: &Value, count: usize) -> Result<(Vec<f64>, usize), Failure> {
    let payload = serde_json::to_vec(body)
        .map_err(|error| Failure::Fatal(format!("unwritable request: {error}")))?;
    let url = format!("{}/v1/systemone", client.base_url.trim_end_matches('/'));
    let mut response = client
        .agent
        .post(url)
        .header("Authorization", format!("Bearer {}", client.key))
        .content_type("application/json")
        .send(&payload[..])
        .map_err(transient_of)?;
    let status = response.status().as_u16();
    let header_of = |name: &str| {
        response
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string)
    };
    let retry_after_ms = header_of("retry-after-ms");
    let retry_after = header_of("retry-after");

    if status == 200 {
        let text = response.body_mut().read_to_string().map_err(transient_of)?;

        return answers_of(&text, count).map_err(Failure::Fatal);
    }

    let text = response.body_mut().read_to_string().unwrap_or_default();

    Err(failure_of(
        status,
        &text,
        retry_after_ms.as_deref(),
        retry_after.as_deref(),
        SystemTime::now(),
    ))
}
