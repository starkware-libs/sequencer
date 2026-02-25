use std::collections::HashSet;
use std::sync::Arc;

use blockifier_reexecution::state_reader::rpc_objects::BlockId;
use jsonrpsee::server::RpcModule;
use jsonrpsee::types::ErrorObjectOwned;
use jsonschema::JSONSchema;
use serde_json::Value;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::EthAddress;
use starknet_api::test_utils::invoke::rpc_invoke_tx;
use starknet_api::transaction::fields::{Proof, ProofFacts};
use starknet_api::transaction::{L2ToL1Payload, MessageToL1};
use starknet_api::{felt, invoke_tx_args};

use crate::config::ProverConfig;
use crate::proving::virtual_snos_prover::{ProveTransactionResult, RpcVirtualSnosProver};
use crate::server::error;
use crate::server::rpc_impl::ProvingRpcServerImpl;
use crate::server::rpc_trait::ProvingRpcServer;

/// Embedded OpenRPC specification document.
const OPENRPC_SPEC: &str = include_str!("../../resources/proving_api_openrpc.json");

fn load_spec() -> Value {
    serde_json::from_str(OPENRPC_SPEC).expect("OpenRPC spec must be valid JSON")
}

/// Transforms spec error definitions into JSON Schemas that can validate error responses.
///
/// Adapted from `apollo_rpc::test_utils::fix_errors`.
fn fix_errors(spec: &mut Value) {
    let Some(errors) = spec
        .as_object_mut()
        .and_then(|obj| obj.get_mut("components"))
        .and_then(|components| components.as_object_mut())
        .and_then(|components| components.get_mut("errors"))
        .and_then(|errors| errors.as_object_mut())
    else {
        return;
    };
    for value in errors.values_mut() {
        let obj = value.as_object_mut().unwrap();
        let Some(code) = obj.get("code").cloned() else {
            continue;
        };
        let Some(message) = obj.get("message").cloned() else {
            continue;
        };
        let has_data = obj.contains_key("data");
        obj.clear();
        let mut properties = serde_json::Map::from_iter([
            (
                "code".to_string(),
                serde_json::Map::from_iter([
                    ("type".to_string(), "integer".into()),
                    ("enum".to_string(), vec![code].into()),
                ])
                .into(),
            ),
            (
                "message".to_string(),
                serde_json::Map::from_iter([
                    ("type".to_string(), "string".into()),
                    ("enum".to_string(), vec![message].into()),
                ])
                .into(),
            ),
        ]);
        let mut required: Vec<Value> = vec!["code".into(), "message".into()];
        if has_data {
            properties.insert("data".to_string(), serde_json::Map::from_iter([]).into());
            required.push("data".into());
        }
        obj.insert("properties".to_string(), properties.into());
        obj.insert("required".to_string(), required.into());
    }
}

/// Compiles a JSON Schema from a `$ref` path within the spec document.
fn compile_schema_for_ref(spec: &Value, ref_path: &str) -> JSONSchema {
    let mut spec_with_errors = spec.clone();
    fix_errors(&mut spec_with_errors);

    let ref_uri = format!("file:///spec#/{ref_path}");
    let ref_schema: Value = serde_json::from_str(&format!(r#"{{"$ref": "{ref_uri}"}}"#)).unwrap();

    JSONSchema::options()
        .with_document("file:///spec".to_string(), spec_with_errors)
        .compile(&ref_schema)
        .expect("Failed to compile schema")
}

/// Compiles a JSON Schema that accepts any of the error definitions for a given method.
fn compile_error_schemas(spec: &Value, method_name: &str) -> JSONSchema {
    let mut spec_with_errors = spec.clone();
    fix_errors(&mut spec_with_errors);

    let methods = spec["methods"].as_array().unwrap();
    let method_index = methods
        .iter()
        .position(|method| method["name"].as_str().unwrap() == method_name)
        .unwrap_or_else(|| panic!("Method {method_name} not found in spec"));

    let errors = methods[method_index]["errors"].as_array().unwrap();
    let mut refs: Vec<String> = Vec::new();
    for (error_index, error_entry) in errors.iter().enumerate() {
        if let Some(ref_str) = error_entry.get("$ref").and_then(|v| v.as_str()) {
            // Inline $ref like "#/components/errors/BLOCK_NOT_FOUND"
            let path = ref_str.trim_start_matches('#').trim_start_matches('/');
            refs.push(format!("file:///spec#/{path}"));
        } else {
            // Inline error definition — reference by method index.
            refs.push(format!("file:///spec#/methods/{method_index}/errors/{error_index}"));
        }
    }

    let any_of: Vec<Value> = refs.iter().map(|r| serde_json::json!({"$ref": r})).collect();
    let ref_schema = serde_json::json!({"anyOf": any_of});

    JSONSchema::options()
        .with_document("file:///spec".to_string(), spec_with_errors)
        .compile(&ref_schema)
        .expect("Failed to compile error schema")
}

fn build_test_rpc_module() -> RpcModule<ProvingRpcServerImpl> {
    let config =
        ProverConfig { rpc_node_url: "http://localhost:1".to_string(), ..Default::default() };
    let rpc_impl = ProvingRpcServerImpl::new(RpcVirtualSnosProver::new(&config), 2);
    rpc_impl.into_rpc()
}

/// Calls a method on the RPC module and returns the raw JSON response.
async fn raw_call(module: &RpcModule<ProvingRpcServerImpl>, method: &str) -> Value {
    let req = format!(r#"{{"jsonrpc":"2.0","id":"1","method":"{method}"}}"#);
    let (resp, _) = module.raw_json_request(&req, 1).await.unwrap();
    serde_json::from_str(resp.get()).unwrap()
}

#[test]
fn spec_methods_match_rpc_module() {
    let module = build_test_rpc_module();
    let module_methods: HashSet<&str> = module.method_names().collect();

    let spec = load_spec();
    let spec_methods: HashSet<&str> = spec["methods"]
        .as_array()
        .unwrap()
        .iter()
        .map(|method| method["name"].as_str().unwrap())
        .collect();

    assert_eq!(module_methods, spec_methods, "Spec methods do not match RPC module methods");
}

#[tokio::test]
async fn spec_version_response_matches_schema() {
    let module = build_test_rpc_module();
    let spec = load_spec();

    let response = raw_call(&module, "starknet_specVersion").await;
    let result = &response["result"];

    let schema = compile_schema_for_ref(&spec, "methods/0/result/schema");
    assert!(
        schema.is_valid(result),
        "starknet_specVersion response does not match spec schema.\nResponse: {}\nErrors: {:?}",
        serde_json::to_string_pretty(result).unwrap(),
        schema.validate(result).err().map(|e| e.collect::<Vec<_>>()),
    );
}

#[test]
fn prove_transaction_result_matches_schema() {
    let spec = load_spec();

    let result = ProveTransactionResult {
        proof: Proof::from(vec![1u32, 2, 3]),
        proof_facts: ProofFacts(Arc::new(vec![felt!("0xdead"), felt!("0xbeef")])),
        l2_to_l1_messages: vec![MessageToL1 {
            from_address: starknet_api::contract_address!("0x123"),
            to_address: EthAddress::try_from(felt!("0xdead")).unwrap(),
            payload: L2ToL1Payload(vec![felt!("0x42")]),
        }],
    };
    let serialized = serde_json::to_value(&result).unwrap();

    let schema = compile_schema_for_ref(&spec, "components/schemas/PROVE_TRANSACTION_RESULT");
    assert!(
        schema.is_valid(&serialized),
        "ProveTransactionResult does not match spec schema.\nSerialized: {}\nErrors: {:?}",
        serde_json::to_string_pretty(&serialized).unwrap(),
        schema.validate(&serialized).err().map(|e| e.collect::<Vec<_>>()),
    );
}

#[test]
fn error_responses_match_schema() {
    let spec = load_spec();
    let error_schema = compile_error_schemas(&spec, "starknet_proveTransaction");

    let test_cases: Vec<(&str, ErrorObjectOwned)> = vec![
        ("block_not_found", error::block_not_found()),
        ("validation_failure", error::validation_failure("test failure".to_string())),
        (
            "unsupported_tx_version",
            error::unsupported_tx_version("version 99 unsupported".to_string()),
        ),
        ("service_busy", error::service_busy(2)),
    ];

    for (label, error) in test_cases {
        let mut map = serde_json::Map::new();
        map.insert("code".to_string(), serde_json::json!(error.code()));
        map.insert("message".to_string(), serde_json::json!(error.message()));
        if let Some(raw_data) = error.data() {
            let data: Value = serde_json::from_str(raw_data.get()).unwrap();
            map.insert("data".to_string(), data);
        }
        let error_value = Value::Object(map);

        assert!(
            error_schema.is_valid(&error_value),
            "Error '{label}' does not match any spec error schema.\nError: {}",
            serde_json::to_string_pretty(&error_value).unwrap(),
        );
    }
}

#[test]
fn spec_parameter_count_matches_methods() {
    let spec = load_spec();
    let methods = spec["methods"].as_array().unwrap();

    let expected_param_counts: Vec<(&str, usize)> =
        vec![("starknet_specVersion", 0), ("starknet_proveTransaction", 2)];

    for (method_name, expected_count) in expected_param_counts {
        let method = methods
            .iter()
            .find(|m| m["name"].as_str().unwrap() == method_name)
            .unwrap_or_else(|| panic!("Method {method_name} not found in spec"));
        let actual_count = method["params"].as_array().unwrap().len();
        assert_eq!(
            actual_count, expected_count,
            "Method {method_name}: expected {expected_count} params, got {actual_count}"
        );
    }
}

#[test]
fn serialized_rpc_transaction_matches_schema() {
    let spec = load_spec();
    let tx_schema = compile_schema_for_ref(&spec, "components/schemas/RPC_TRANSACTION");

    // Use non-empty proof and proof_facts so those fields are serialized and validated.
    let transaction = rpc_invoke_tx(invoke_tx_args!(
        proof: Proof::from(vec![1u32, 2, 3]),
        proof_facts: ProofFacts(Arc::new(vec![felt!("0xdead"), felt!("0xbeef")])),
    ));
    let serialized = serde_json::to_value(&transaction).unwrap();

    assert!(
        tx_schema.is_valid(&serialized),
        "Serialized RpcTransaction does not match RPC_TRANSACTION schema.\nSerialized: \
         {}\nErrors: {:?}",
        serde_json::to_string_pretty(&serialized).unwrap(),
        tx_schema.validate(&serialized).err().map(|e| e.collect::<Vec<_>>()),
    );
}

#[test]
fn serialized_block_id_matches_schema() {
    let spec = load_spec();
    let block_id_schema = compile_schema_for_ref(&spec, "components/schemas/BLOCK_ID");

    let test_cases: Vec<(&str, BlockId)> = vec![
        ("hash", BlockId::Hash(BlockHash(felt!("0x123")))),
        ("number", BlockId::Number(BlockNumber(42))),
        ("latest", BlockId::Latest),
    ];

    for (label, block_id) in test_cases {
        let serialized = serde_json::to_value(block_id).unwrap();
        assert!(
            block_id_schema.is_valid(&serialized),
            "BlockId::{label} does not match BLOCK_ID schema.\nSerialized: {}\nErrors: {:?}",
            serde_json::to_string_pretty(&serialized).unwrap(),
            block_id_schema.validate(&serialized).err().map(|e| e.collect::<Vec<_>>()),
        );
    }
}
