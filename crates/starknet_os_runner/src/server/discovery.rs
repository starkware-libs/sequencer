//! OpenRPC discovery endpoint for the proving service.
//!
//! Registers the `rpc.discover` method which returns the OpenRPC specification document.
//! Uses manual `RpcModule::register_method` instead of the `#[rpc]` macro because
//! the method name contains a dot separator that the macro's namespace handling
//! cannot produce.

use jsonrpsee::RpcModule;
use serde_json::Value;

/// Embedded OpenRPC specification document.
const OPENRPC_SPEC: &str = include_str!("../../resources/proving_api_openrpc.json");

/// Creates an `RpcModule` that serves the `rpc.discover` method.
///
/// The spec is parsed once at construction time and cloned per request.
pub fn discovery_rpc_module() -> RpcModule<()> {
    let spec: Value =
        serde_json::from_str(OPENRPC_SPEC).expect("Embedded OpenRPC spec must be valid JSON");
    let mut module = RpcModule::new(());
    module
        .register_method("rpc.discover", move |_params, _context, _extensions| spec.clone())
        .expect("Failed to register rpc.discover method");
    module
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openrpc_spec_is_valid_json() {
        let spec: Value = serde_json::from_str(OPENRPC_SPEC).unwrap();
        assert_eq!(spec["openrpc"], "1.0.0-rc1");
        assert_eq!(spec["info"]["title"], "Starknet OS Runner Proving API");
    }

    #[test]
    fn discovery_module_registers_method() {
        let module = discovery_rpc_module();
        let method_names: Vec<&str> = module.method_names().collect();
        assert!(method_names.contains(&"rpc.discover"), "rpc.discover not found in {method_names:?}");
    }
}
