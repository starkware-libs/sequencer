use std::ffi::OsString;

use rstest::rstest;

use crate::server::log_redact::{redact_url_host, text_log_ansi_enabled};

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

#[rstest]
#[case::terminal_without_no_color(true, None, true)]
#[case::terminal_with_empty_no_color(true, Some(""), true)]
#[case::terminal_with_no_color(true, Some("1"), false)]
#[case::piped_without_no_color(false, None, false)]
#[case::piped_with_no_color(false, Some("1"), false)]
fn text_log_ansi_enabled_requires_terminal_and_unset_no_color(
    #[case] stdout_is_terminal: bool,
    #[case] no_color: Option<&str>,
    #[case] expected_ansi_enabled: bool,
) {
    assert_eq!(
        text_log_ansi_enabled(stdout_is_terminal, no_color.map(OsString::from)),
        expected_ansi_enabled
    );
}
