use crate::secrets::{Sensitive, DEFAULT_REDACTION_OUTPUT};

#[test]
fn test_default_redaction_output() {
    let sensitive = Sensitive::new("secret");
    assert_eq!(sensitive.redact(), DEFAULT_REDACTION_OUTPUT);
}

#[test]
fn test_custom_redaction_without_args() {
    let redactor = |_: &String| "censored".to_string();
    let sensitive = Sensitive::new("secret".to_string()).with_redactor(redactor);
    assert_eq!(sensitive.redact(), "censored");
}

#[test]
fn test_custom_redaction_with_args() {
    let redactor = |s: &String| s.chars().take(2).collect::<String>();
    let sensitive = Sensitive::new("abcdefgh".to_string()).with_redactor(redactor);
    assert_eq!(sensitive.redact(), "ab");
}

#[test]
fn test_debug_display_serialize() {
    let sensitive = Sensitive::new("secret");
    assert_eq!(format!("{:?}", sensitive), DEFAULT_REDACTION_OUTPUT);
    assert_eq!(format!("{}", sensitive), DEFAULT_REDACTION_OUTPUT);
    assert_eq!(
        serde_json::to_string(&sensitive).unwrap(),
        serde_json::to_string(&DEFAULT_REDACTION_OUTPUT).unwrap()
    );
}

#[test]
fn test_into() {
    let sensitive = Sensitive::new("secret");
    assert_eq!(sensitive.into(), "secret");
}

#[test]
fn test_ref() {
    let sensitive = Sensitive::new("secret");
    assert_eq!(sensitive.as_ref(), &"secret");
}

#[test]
fn test_mut() {
    let mut sensitive = Sensitive::new("secret".to_string());
    let inner = sensitive.as_mut();
    inner.push_str("123");
    assert_eq!(sensitive.into(), "secret123".to_string());
}
