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
fn the_body_is_sent_as_json() {
    let mock = MockJev::start(|_| {
        (
            200,
            Vec::new(),
            r#"{"answers":{"L0":{"type":"noul","noul":0.5}}}"#.to_string(),
        )
    });

    assert_eq!(outcome_of(mock.url.clone()), Ok((vec![0.5], 0)));
    assert_eq!(
        mock.requests.lock().unwrap()[0].header_of("content-type"),
        Some("application/json")
    );
}

#[test]
fn a_200_whose_whole_body_is_unreadable_is_fatal() {
    let mock = MockJev::start(|_| (200, Vec::new(), "<html>busy</html>".to_string()));
    let outcome = outcome_of(mock.url.clone());

    assert!(
        matches!(&outcome, Err(Failure::Fatal(message)) if message.starts_with("unreadable response: ")),
        "{outcome:?}"
    );
}

#[test]
fn a_200_whose_body_fails_to_arrive_is_transient() {
    let mock = MockJev::start_raw(|_| {
        b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\n{\"answers\"".to_vec()
    });

    assert!(failure_message_of(outcome_of(mock.url.clone())).starts_with("request failed: "));
}

#[test]
fn an_error_response_whose_body_fails_to_arrive_is_transient() {
    let mock = MockJev::start_raw(|_| {
        b"HTTP/1.1 400 Bad Request\r\nContent-Length: 100\r\nConnection: close\r\n\r\n{\"detail\""
            .to_vec()
    });

    assert!(failure_message_of(outcome_of(mock.url.clone())).starts_with("request failed: "));
}

#[test]
fn an_error_body_that_is_not_utf8_is_read_lossily() {
    let mock = MockJev::start_raw(|_| {
        b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 7\r\nConnection: close\r\n\r\n\xff\xfe busy"
            .to_vec()
    });

    assert_eq!(
        outcome_of(mock.url.clone()),
        Err(Failure::Transient {
            message: "HTTP 503: \u{fffd}\u{fffd} busy".to_string(),
            retry_after: None
        })
    );
}

#[test]
fn a_200_whose_body_is_not_utf8_is_fatal() {
    let mock = MockJev::start_raw(|_| {
        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n\xff\xfe".to_vec()
    });
    let outcome = outcome_of(mock.url.clone());

    assert!(
        matches!(&outcome, Err(Failure::Fatal(message)) if message.starts_with("unreadable response: ")),
        "{outcome:?}"
    );
}

#[test]
fn a_rate_limit_whose_body_fails_to_arrive_still_holds() {
    let mock = MockJev::start_raw(|_| {
        b"HTTP/1.1 429 Too Many Requests\r\nretry-after-ms: 50\r\nContent-Length: 100\r\nConnection: close\r\n\r\nshort"
            .to_vec()
    });

    assert_eq!(
        outcome_of(mock.url.clone()),
        Err(Failure::RateLimited {
            retry_after: Some(Duration::from_millis(50))
        })
    );
}

#[test]
fn a_malformed_response_is_a_transient_protocol_error() {
    let mock = MockJev::start_raw(|_| b"NOT HTTP AT ALL\r\n\r\n".to_vec());

    assert!(
        failure_message_of(outcome_of(mock.url.clone())).starts_with("request failed: protocol: ")
    );
}

#[test]
fn a_tls_handshake_with_a_plain_http_server_is_fatal() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();

    std::thread::spawn(move || {
        use std::io::{Read, Write};

        let (mut stream, _) = listener.accept().unwrap();
        let mut hello = [0; 5];

        let _ = stream.read_exact(&mut hello);
        let _ = stream.write_all(
            b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        );
        let _ = stream.flush();
        let _ = stream.read_to_end(&mut Vec::new());
    });

    let outcome = outcome_of(format!("https://{address}"));

    assert!(
        matches!(&outcome, Err(Failure::Fatal(message)) if message.starts_with("request failed: ")),
        "{outcome:?}"
    );
}
