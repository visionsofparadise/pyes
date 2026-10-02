use std::time::{Duration, SystemTime};

use serde_json::Value;

use crate::answers_of::{answers_of, failure_of, Failure};

pub const BASE_URL: &str = "https://api.typesafe.ai";
pub const TIMEOUT: Duration = Duration::from_secs(30);

#[cfg(windows)]
const EXHAUSTION_ERRORS: [i32; 2] = [10024, 10055];
#[cfg(not(windows))]
const EXHAUSTION_ERRORS: [i32; 2] = [24, 23];

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
            .proxy(None)
            .max_redirects(0)
            .build()
            .into();

        Client {
            agent,
            base_url,
            key,
        }
    }
}

fn transport_failure_of(error: ureq::Error) -> Failure {
    let message = format!("request failed: {error}");

    match error {
        ureq::Error::Io(_)
        | ureq::Error::Timeout(_)
        | ureq::Error::HostNotFound
        | ureq::Error::ConnectionFailed
        | ureq::Error::Protocol(_)
        | ureq::Error::ConnectProxyFailed(_)
        | ureq::Error::Decompress(..) => Failure::Transient {
            message,
            retry_after: None,
        },
        _ => Failure::Fatal(message),
    }
}

fn request_failure_of(error: ureq::Error) -> Failure {
    match error {
        ureq::Error::Io(ref cause)
            if cause
                .raw_os_error()
                .is_some_and(|code| EXHAUSTION_ERRORS.contains(&code)) =>
        {
            Failure::Exhausted(format!("request failed: {error}"))
        }
        ureq::Error::Io(ref cause) if cause.kind() == std::io::ErrorKind::InvalidData => {
            Failure::Fatal(format!("request failed: {error}"))
        }
        _ => transport_failure_of(error),
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
        .map_err(request_failure_of)?;
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
    let status_failure_of = |text: &str| {
        failure_of(
            status,
            text,
            retry_after_ms.as_deref(),
            retry_after.as_deref(),
            SystemTime::now(),
        )
    };

    if status == 429 {
        return Err(status_failure_of(""));
    }

    let bytes = response
        .body_mut()
        .read_to_vec()
        .map_err(transport_failure_of)?;
    let text = String::from_utf8_lossy(&bytes);

    if status == 200 {
        return answers_of(&text, count).map_err(Failure::Fatal);
    }

    Err(status_failure_of(&text))
}

#[cfg(test)]
#[path = "attempt.test.rs"]
mod tests;

#[cfg(test)]
#[path = "attempt.integration.test.rs"]
mod integration;
