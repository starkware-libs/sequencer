use validator::Validate;

use crate::component_client::RemoteClientConfig;

// keepalive_timeout_ms = 2000 ms: tcp_raw = 3000 ms, tcp_whole_secs = 3 s = 3000 ms, which is
// strictly greater than http_keepalive = 2000 ms.
#[test]
fn tcp_keepalive_validation_passes_when_tcp_strictly_greater_than_http_keepalive() {
    let config = RemoteClientConfig { keepalive_timeout_ms: 2000, ..Default::default() };
    assert!(config.validate().is_ok());
}

// keepalive_timeout_ms = 1000 ms: tcp_raw = 1500 ms, tcp_whole_secs = 1 s = 1000 ms, which equals
// http_keepalive = 1000 ms.
#[test]
fn tcp_keepalive_validation_passes_when_tcp_equals_http_keepalive() {
    let config = RemoteClientConfig { keepalive_timeout_ms: 1000, ..Default::default() };
    assert!(config.validate().is_ok());
}

// keepalive_timeout_ms = 1100 ms: tcp_raw = 1650 ms, tcp_whole_secs = 1 s = 1000 ms, which is
// less than http_keepalive = 1100 ms.
#[test]
fn tcp_keepalive_validation_fails_when_tcp_truncated_below_http_keepalive() {
    let config = RemoteClientConfig { keepalive_timeout_ms: 1100, ..Default::default() };
    assert!(config.validate().is_err());
}

#[test]
fn tcp_keepalive_validation_fails_when_keepalive_timeout_ms_is_zero() {
    let config = RemoteClientConfig { keepalive_timeout_ms: 0, ..Default::default() };
    assert!(config.validate().is_err());
}
