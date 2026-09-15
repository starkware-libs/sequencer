use std::panic::{panic_any, resume_unwind};

use super::catch_panic_with_location;
use crate::server::panic::{install_panic_hook, take_last_panic_location};

#[test]
fn captures_message_and_location() {
    install_panic_hook();
    let expected_line = line!() + 1;
    let error = catch_panic_with_location(|| panic!("boom")).unwrap_err();
    let location = error.location.unwrap();
    assert_eq!(error.message, "boom");
    assert!(location.file.ends_with("panic_capture_test.rs"));
    assert_eq!(location.line, expected_line);
}

#[test]
fn returns_values_and_clears_location() {
    install_panic_hook();
    let result = catch_panic_with_location(|| {
        let _ = std::panic::catch_unwind(|| panic!("handled internally"));
        7
    });
    assert_eq!(result.unwrap(), 7);
    assert!(take_last_panic_location().is_none());
}

#[test]
fn captures_formatted_and_non_string_payloads() {
    install_panic_hook();
    assert_eq!(catch_panic_with_location(|| panic!("{}", 42)).unwrap_err().message, "42");
    assert_eq!(
        catch_panic_with_location(|| panic_any(42)).unwrap_err().message,
        "non-string panic payload"
    );
}

#[test]
fn does_not_reuse_an_earlier_location() {
    install_panic_hook();
    let _ = std::panic::catch_unwind(|| panic!("earlier"));
    let error = catch_panic_with_location(|| resume_unwind(Box::new("resumed"))).unwrap_err();
    assert!(error.location.is_none());
}

#[test]
fn nested_capture_does_not_attribute_inner_location_to_outer_panic() {
    install_panic_hook();
    let error = catch_panic_with_location(|| {
        let _ = catch_panic_with_location(|| panic!("inner"));
        resume_unwind(Box::new("outer"));
    })
    .unwrap_err();
    assert!(error.location.is_none());
    assert_eq!(error.message, "outer");
}
