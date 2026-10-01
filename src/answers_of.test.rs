use super::*;

const NOW: SystemTime = SystemTime::UNIX_EPOCH;

fn fail(status: u16, body: &str) -> Failure {
    failure_of(status, body, None, None, NOW)
}

#[test]
fn reads_answers_in_order_with_usage() {
    let body = r#"{"model":"jev-1.13.0","answers":{"L0":{"type":"noul","noul":0.12},"L1":{"type":"noul","noul":0.9}},"usage":{"input_tokens":313,"output_tokens":21}}"#;

    assert_eq!(answers_of(body, 2), Ok((vec![0.12, 0.9], 313)));
}

#[test]
fn usage_defaults_to_zero() {
    assert_eq!(
        answers_of(r#"{"answers":{"L0":{"noul":1}}}"#, 1),
        Ok((vec![1.0], 0))
    );
}

#[test]
fn names_the_missing_answer() {
    let missing = Err("response has no answer for L1".to_string());

    assert_eq!(
        answers_of(r#"{"answers":{"L0":{"noul":0.5},"L1":{"type":"noul"}}}"#, 2),
        missing
    );
    assert_eq!(answers_of(r#"{"answers":{"L0":{"noul":0.5}}}"#, 2), missing);
}

#[test]
fn malformed_json_is_unreadable() {
    let error = answers_of("<html>", 1).unwrap_err();

    assert!(error.starts_with("unreadable response: "));
}

#[test]
fn max_tokens_exceeded_is_too_large() {
    assert_eq!(
        fail(400, r#"{"detail":{"error_type":"max_tokens_exceeded"}}"#),
        Failure::TooLarge
    );
}

#[test]
fn another_400_error_type_is_fatal() {
    assert_eq!(
        fail(
            400,
            r#"{"detail":{"error_type":"bad_state","message":"state is invalid"}}"#
        ),
        Failure::Fatal("HTTP 400: state is invalid".to_string())
    );
    assert_eq!(
        fail(400, r#"{"detail":{"error_type":"bad_state"}}"#),
        Failure::Fatal("HTTP 400: bad_state".to_string())
    );
}

#[test]
fn a_string_detail_is_the_message() {
    assert_eq!(
        fail(401, r#"{"detail":"Invalid API key"}"#),
        Failure::Fatal("HTTP 401: Invalid API key".to_string())
    );
}

#[test]
fn a_422_list_joins_msgs_and_omits_input() {
    let body = r#"{"detail":[{"type":"missing","loc":["body","model"],"msg":"Field required","input":{"state":"SECRET"}},{"type":"x","loc":[],"msg":"Other problem","input":1}]}"#;

    assert_eq!(
        fail(422, body),
        Failure::Fatal("HTTP 422: Field required; Other problem".to_string())
    );
}

#[test]
fn a_422_list_without_msgs_joins_types_and_omits_input() {
    let body = r#"{"detail":[{"type":"missing","input":{"state":"SECRET"}},{"type":"extra_forbidden","input":"SECRET"}]}"#;

    assert_eq!(
        fail(422, body),
        Failure::Fatal("HTTP 422: missing; extra_forbidden".to_string())
    );
    assert_eq!(
        fail(422, r#"{"detail":[{"input":"SECRET"}]}"#),
        Failure::Fatal("HTTP 422: unprocessable request".to_string())
    );
}

#[test]
fn a_non_json_body_is_the_message() {
    assert_eq!(
        fail(404, "not found\n"),
        Failure::Fatal("HTTP 404: not found".to_string())
    );
}

#[test]
fn rate_limit_versus_unavailable() {
    assert_eq!(fail(429, "{}"), Failure::RateLimited { retry_after: None });
    assert_eq!(
        fail(503, r#"{"detail":"overloaded"}"#),
        Failure::Transient {
            message: "HTTP 503: overloaded".to_string(),
            retry_after: None
        }
    );
    assert!(matches!(fail(408, "{}"), Failure::Transient { .. }));
    assert!(matches!(fail(500, "{}"), Failure::Transient { .. }));
}

#[test]
fn retry_after_ms_wins_over_seconds() {
    assert_eq!(
        failure_of(429, "{}", Some("250"), Some("5"), NOW),
        Failure::RateLimited {
            retry_after: Some(Duration::from_millis(250))
        }
    );
    assert_eq!(
        failure_of(429, "{}", None, Some("5"), NOW),
        Failure::RateLimited {
            retry_after: Some(Duration::from_secs(5))
        }
    );
}

#[test]
fn retry_after_resolves_an_http_date_against_now() {
    let now = httpdate::parse_http_date("Wed, 21 Oct 2015 07:28:00 GMT").unwrap();

    assert_eq!(
        failure_of(
            503,
            r#"{"detail":"busy"}"#,
            None,
            Some("Wed, 21 Oct 2015 07:28:30 GMT"),
            now
        ),
        Failure::Transient {
            message: "HTTP 503: busy".to_string(),
            retry_after: Some(Duration::from_secs(30))
        }
    );
}

#[test]
fn retry_after_over_sixty_seconds_is_dropped() {
    assert_eq!(
        failure_of(429, "{}", None, Some("61"), NOW),
        Failure::RateLimited { retry_after: None }
    );
    assert_eq!(
        failure_of(429, "{}", Some("60001"), None, NOW),
        Failure::RateLimited { retry_after: None }
    );
    assert_eq!(
        failure_of(429, "{}", Some("60000"), None, NOW),
        Failure::RateLimited {
            retry_after: Some(Duration::from_secs(60))
        }
    );
}

#[test]
fn huge_values_are_dropped_without_panicking() {
    assert_eq!(
        failure_of(429, "{}", None, Some("1e20"), NOW),
        Failure::RateLimited { retry_after: None }
    );
    assert_eq!(
        failure_of(429, "{}", Some("1e23"), None, NOW),
        Failure::RateLimited { retry_after: None }
    );
}

#[test]
fn an_over_limit_retry_after_ms_does_not_fall_through_to_seconds() {
    assert_eq!(
        failure_of(429, "{}", Some("70000"), Some("5"), NOW),
        Failure::RateLimited { retry_after: None }
    );
}

#[test]
fn an_unparseable_or_negative_retry_after_ms_defers_to_seconds() {
    assert_eq!(
        failure_of(429, "{}", Some("soon"), Some("5"), NOW),
        Failure::RateLimited {
            retry_after: Some(Duration::from_secs(5))
        }
    );
    assert_eq!(
        failure_of(429, "{}", Some("-1"), Some("5"), NOW),
        Failure::RateLimited {
            retry_after: Some(Duration::from_secs(5))
        }
    );
}
