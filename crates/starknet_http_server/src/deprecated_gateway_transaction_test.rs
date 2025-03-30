use crate::test_utils::{
    deprecated_gateway_declare_tx,
    deprecated_gateway_deploy_account_tx,
    deprecated_gateway_invoke_tx,
};

#[test]
fn deprecated_gateway_invoke_tx_deserialization() {
    // TODO(Arni): use this transaction in the HTTP server's positive flow test, and delete this
    // test.
    let _ = deprecated_gateway_invoke_tx();
}

#[test]
fn deprecated_gateway_deploy_account_tx_deserialization() {
    // TODO(Arni): use this transaction in the HTTP server's positive flow test, and delete this
    // test.
    let _ = deprecated_gateway_deploy_account_tx();
}

#[test]
fn deprecated_gateway_declare_tx_deserialization() {
    // TODO(Arni): use this transaction in the HTTP server's positive flow test, and delete this
    // test.
    let _ = deprecated_gateway_declare_tx();
}
