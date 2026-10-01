use std::time::{Duration, SystemTime};

use serde_json::Value;

const RETRY_AFTER_LIMIT: Duration = Duration::from_secs(60);

#[derive(Debug, PartialEq)]
pub enum Failure {
    TooLarge,
    RateLimited {
        retry_after: Option<Duration>,
    },
    Transient {
        message: String,
        retry_after: Option<Duration>,
    },
    Fatal(String),
}

pub fn answers_of(body: &str, count: usize) -> Result<(Vec<f64>, usize), String> {
    let parsed: Value =
        serde_json::from_str(body).map_err(|error| format!("unreadable response: {error}"))?;
    let mut answers = Vec::with_capacity(count);

    for index in 0..count {
        let noul = parsed["answers"][format!("L{index}")]["noul"]
            .as_f64()
            .ok_or_else(|| format!("response has no answer for L{index}"))?;

        answers.push(noul);
    }

    let tokens = parsed["usage"]["input_tokens"].as_u64().unwrap_or(0);

    Ok((answers, tokens as usize))
}

fn duration_of(seconds: f64) -> Option<Duration> {
    if !seconds.is_finite() || seconds < 0.0 || seconds > RETRY_AFTER_LIMIT.as_secs_f64() {
        return None;
    }

    Some(Duration::from_secs_f64(seconds))
}

fn retry_after_of(
    retry_after_ms: Option<&str>,
    retry_after: Option<&str>,
    now: SystemTime,
) -> Option<Duration> {
    let milliseconds = retry_after_ms
        .and_then(|text| text.trim().parse::<f64>().ok())
        .filter(|milliseconds| milliseconds.is_finite() && *milliseconds >= 0.0);

    if let Some(milliseconds) = milliseconds {
        return duration_of(milliseconds / 1000.0);
    }

    let text = retry_after?.trim();

    match text.parse::<f64>() {
        Ok(seconds) => duration_of(seconds),
        Err(_) => {
            let date = httpdate::parse_http_date(text).ok()?;

            duration_of(date.duration_since(now).unwrap_or_default().as_secs_f64())
        }
    }
}

fn message_of(parsed: &Value, body: &str) -> String {
    let detail = &parsed["detail"];

    if let Some(message) = detail["message"].as_str() {
        return message.to_string();
    }

    if let Some(message) = detail.as_str() {
        return message.to_string();
    }

    if let Some(items) = detail.as_array() {
        let joined = |key: &str| {
            items
                .iter()
                .filter_map(|item| item[key].as_str())
                .collect::<Vec<&str>>()
                .join("; ")
        };

        return [joined("msg"), joined("type")]
            .into_iter()
            .find(|text| !text.is_empty())
            .unwrap_or_else(|| "unprocessable request".to_string());
    }

    if let Some(error_type) = detail["error_type"].as_str() {
        return error_type.to_string();
    }

    body.trim().to_string()
}

pub fn failure_of(
    status: u16,
    body: &str,
    retry_after_ms: Option<&str>,
    retry_after: Option<&str>,
    now: SystemTime,
) -> Failure {
    let parsed: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    let retry_after = retry_after_of(retry_after_ms, retry_after, now);
    let described = || format!("HTTP {status}: {}", message_of(&parsed, body));

    match status {
        400 if parsed["detail"]["error_type"] == "max_tokens_exceeded" => Failure::TooLarge,
        429 => Failure::RateLimited { retry_after },
        408 | 500..=599 => Failure::Transient {
            message: described(),
            retry_after,
        },
        _ => Failure::Fatal(described()),
    }
}

#[cfg(test)]
#[path = "answers_of.test.rs"]
mod tests;
