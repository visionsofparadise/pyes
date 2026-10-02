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
fn the_agent_times_out_at_30_s_and_uses_no_proxy() {
    let client = Client::new("http://127.0.0.1:1".to_string(), "key".to_string());
    let config = client.agent.config();

    assert_eq!(config.timeouts().global, Some(TIMEOUT));
    assert!(config.proxy().is_none());
}
