use rstest::rstest;

use super::is_permanent_http_error;

#[rstest]
#[case::not_found("curl: (22) The requested URL returned error: 404\n", true)]
#[case::not_found_with_reason("curl: (22) The requested URL returned error: 404 Not Found\n", true)]
#[case::forbidden("curl: (22) The requested URL returned error: 403\n", true)]
#[case::request_timeout("curl: (22) The requested URL returned error: 408\n", false)]
#[case::too_many_requests("curl: (22) The requested URL returned error: 429\n", false)]
#[case::server_error("curl: (22) The requested URL returned error: 503\n", false)]
#[case::stalled_transfer(
    "curl: (28) Operation too slow. Less than 1024 bytes/sec transferred the last 20 seconds\n",
    false
)]
#[case::empty("", false)]
fn is_permanent_http_error_classifies_curl_stderr(
    #[case] curl_stderr: &str,
    #[case] expected_is_permanent: bool,
) {
    assert_eq!(is_permanent_http_error(curl_stderr), expected_is_permanent);
}
