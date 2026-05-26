use crate::server::log_redact::redact_url_host;

#[test]
fn strips_userinfo_path_and_query() {
    assert_eq!(
        redact_url_host("https://user:pass@rpc.example.com:8443/v1?token=abc"),
        "https://rpc.example.com:8443"
    );
}

#[test]
fn keeps_default_port_implicit() {
    assert_eq!(redact_url_host("https://rpc.example.com/"), "https://rpc.example.com");
}

#[test]
fn returns_placeholder_for_invalid_url() {
    assert_eq!(redact_url_host("not a url"), "<invalid url>");
}

#[test]
fn returns_placeholder_for_empty_string() {
    // Pinned so callers know empty input lands in the invalid-url path and
    // can guard with `<unset>` at the call site when that's misleading.
    assert_eq!(redact_url_host(""), "<invalid url>");
}

#[test]
fn drops_fragment() {
    assert_eq!(redact_url_host("https://rpc.example.com/#secret"), "https://rpc.example.com");
}
