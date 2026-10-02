use super::*;
use crate::mock_jev::MockJev;
use serde_json::json;

fn body_of() -> Value {
    json!({ "questions": {}, "state": { "lines": {} } })
}

fn outcome_of(base_url: String) -> Result<(Vec<f64>, usize), Failure> {
    attempt(&Client::new(base_url, "test".to_string()), &body_of(), 1)
}

fn failure_message_of(outcome: Result<(Vec<f64>, usize), Failure>) -> String {
    match outcome {
        Err(Failure::Transient {
            message,
            retry_after: None,
        }) => message,
        other => panic!("expected a transient failure, got {other:?}"),
    }
}

#[test]
fn a_trailing_slash_on_the_base_url_is_trimmed() {
    let mock = MockJev::start(|_| {
        (
            200,
            Vec::new(),
            r#"{"answers":{"L0":{"type":"noul","noul":0.5}},"usage":{"input_tokens":3}}"#
                .to_string(),
        )
    });

    assert_eq!(outcome_of(format!("{}/", mock.url)), Ok((vec![0.5], 3)));
    assert_eq!(mock.requests.lock().unwrap()[0].path, "/v1/systemone");
}

#[test]
fn a_200_whose_body_fails_to_arrive_is_transient() {
    let mock = MockJev::start_raw(|_| {
        b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\n{\"answers\"".to_vec()
    });

    assert!(failure_message_of(outcome_of(mock.url.clone())).starts_with("request failed: "));
}

#[test]
fn a_malformed_response_is_a_transient_protocol_error() {
    let mock = MockJev::start_raw(|_| b"NOT HTTP AT ALL\r\n\r\n".to_vec());

    assert!(
        failure_message_of(outcome_of(mock.url.clone())).starts_with("request failed: protocol: ")
    );
}
