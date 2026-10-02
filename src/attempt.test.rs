use super::*;
use std::io;

#[test]
fn network_errors_are_transient() {
    for error in [
        ureq::Error::Io(io::Error::new(io::ErrorKind::ConnectionReset, "reset")),
        ureq::Error::Timeout(ureq::Timeout::Global),
        ureq::Error::HostNotFound,
        ureq::Error::ConnectionFailed,
        ureq::Error::ConnectProxyFailed("refused".to_string()),
        ureq::Error::Decompress(
            "gzip",
            io::Error::new(io::ErrorKind::UnexpectedEof, "truncated"),
        ),
    ] {
        let message = format!("request failed: {error}");

        assert_eq!(
            request_failure_of(error),
            Failure::Transient {
                message,
                retry_after: None
            }
        );
    }
}

#[test]
fn errors_a_retry_cannot_change_are_fatal() {
    for error in [
        ureq::Error::BadUri("no host".to_string()),
        ureq::Error::Tls("no provider"),
        ureq::Error::TooManyRedirects,
        ureq::Error::RedirectFailed,
    ] {
        let message = format!("request failed: {error}");

        assert_eq!(request_failure_of(error), Failure::Fatal(message));
    }
}

#[test]
fn a_tls_verdict_is_fatal() {
    let error = ureq::Error::Io(io::Error::new(
        io::ErrorKind::InvalidData,
        "invalid peer certificate: UnknownIssuer",
    ));
    let message = format!("request failed: {error}");

    assert_eq!(request_failure_of(error), Failure::Fatal(message));
}

#[test]
fn invalid_data_while_reading_a_body_is_transient() {
    let error = ureq::Error::Io(io::Error::new(io::ErrorKind::InvalidData, "bad record"));
    let message = format!("request failed: {error}");

    assert_eq!(
        transport_failure_of(error),
        Failure::Transient {
            message,
            retry_after: None
        }
    );
}

#[test]
fn the_agent_times_out_at_30_s_and_uses_no_proxy() {
    let client = Client::new("http://127.0.0.1:1".to_string(), "key".to_string());
    let config = client.agent.config();

    assert_eq!(config.timeouts().global, Some(TIMEOUT));
    assert!(config.proxy().is_none());
}

#[test]
fn exhausted_local_resources_are_named_as_exhaustion() {
    let codes: [i32; 2] = if cfg!(windows) {
        [10024, 10055]
    } else {
        [24, 23]
    };

    for code in codes {
        let error = ureq::Error::Io(io::Error::from_raw_os_error(code));
        let message = format!("request failed: {error}");

        assert_eq!(request_failure_of(error), Failure::Exhausted(message));
    }
}
