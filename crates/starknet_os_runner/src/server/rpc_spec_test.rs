use std::collections::HashSet;
use std::sync::LazyLock;

use blockifier_reexecution::state_reader::rpc_objects::BlockId;
use jsonrpsee::server::RpcModule;
use jsonrpsee::types::error::INVALID_PARAMS_CODE;
use jsonrpsee::types::ErrorObjectOwned;
use jsonschema::JSONSchema;
use rstest::rstest;
use serde_json::Value;
use starknet_api::core::EthAddress;
use starknet_api::test_utils::invoke::rpc_invoke_tx;
use starknet_api::test_utils::read_json_file;
use starknet_api::transaction::fields::{Proof, ProofFacts};
use starknet_api::transaction::{L2ToL1Payload, MessageToL1};
use starknet_api::{felt, invoke_tx_args};

use crate::config::ProverConfig;
use crate::proving::virtual_snos_prover::{ProveTransactionResult, RpcVirtualSnosProver};
use crate::server::error::RpcError;
use crate::server::rpc_impl::ProvingRpcServerImpl;
use crate::server::rpc_trait::ProvingRpcServer;

const SPEC_VERSION_METHOD: &str = "starknet_specVersion";
const PROVE_TRANSACTION_METHOD: &str = "starknet_proveTransaction";

static SPEC: LazyLock<Value> = LazyLock::new(|| read_json_file("proving_api_openrpc.json"));

#[rstest::fixture]
fn rpc_module() -> RpcModule<ProvingRpcServerImpl> {
    let config =
        ProverConfig { rpc_node_url: "http://localhost:1".to_string(), ..Default::default() };
    let rpc_impl = ProvingRpcServerImpl::new(RpcVirtualSnosProver::new(&config), 2);
    rpc_impl.into_rpc()
}

/// Compiles a JSON Schema from a `$ref` path within the spec document.
fn compile_schema_for_ref(spec: &Value, ref_path: &str) -> JSONSchema {
    let ref_uri = format!("file:///spec#/{ref_path}");
    let ref_schema: Value = serde_json::from_str(&format!(r#"{{"$ref": "{ref_uri}"}}"#)).unwrap();

    JSONSchema::options()
        .with_document("file:///spec".to_string(), spec.clone())
        .compile(&ref_schema)
        .expect("Failed to compile schema")
}

fn assert_matches_schema(schema: &JSONSchema, value: &Value, label: &str) {
    assert!(
        schema.is_valid(value),
        "{label} does not match spec schema.\nSerialized: {}\nErrors: {:?}",
        serde_json::to_string_pretty(value).unwrap(),
        schema.validate(value).err().map(|e| e.collect::<Vec<_>>()),
    );
}

struct SpecError {
    code: i32,
    message: String,
    expects_data: bool,
}

impl SpecError {
    fn from_spec(value: &Value) -> Self {
        Self {
            code: i32::try_from(value["code"].as_i64().unwrap()).unwrap(),
            message: value["message"].as_str().unwrap().to_string(),
            expects_data: value.get("data").is_some(),
        }
    }

    fn assert_matches(&self, error: &ErrorObjectOwned) {
        assert_eq!(error.code(), self.code);
        assert_eq!(error.message(), self.message);
        assert_eq!(error.data().is_some(), self.expects_data);
    }
}

fn sample_prove_transaction_result() -> ProveTransactionResult {
    ProveTransactionResult {
        proof: Proof::proof_for_testing(),
        proof_facts: ProofFacts::snos_proof_facts_for_testing(),
        l2_to_l1_messages: vec![MessageToL1 {
            from_address: starknet_api::contract_address!("0x123"),
            to_address: EthAddress::try_from(felt!("0xdead")).unwrap(),
            payload: L2ToL1Payload(vec![felt!("0x42")]),
        }],
    }
}

/// Builds test cases with resolved spec data for each RPC method.
/// Also asserts that the test cases cover every method in the spec and module (completeness guard).
fn build_method_test_cases(
    module: &RpcModule<ProvingRpcServerImpl>,
) -> Vec<(&'static str, usize, Vec<Value>)> {
    let sample_values: Vec<(&str, Vec<Value>)> = vec![
        (SPEC_VERSION_METHOD, vec![]),
        (
            PROVE_TRANSACTION_METHOD,
            vec![
                serde_json::to_value(BlockId::Latest).unwrap(),
                serde_json::to_value(rpc_invoke_tx(invoke_tx_args!())).unwrap(),
            ],
        ),
    ];

    // Completeness guard: test cases must cover all spec and module methods.
    let methods = SPEC["methods"].as_array().unwrap();
    let tested: HashSet<&str> = sample_values.iter().map(|(name, _)| *name).collect();
    let spec_methods: HashSet<&str> = methods.iter().map(|m| m["name"].as_str().unwrap()).collect();
    let module_methods: HashSet<&str> = module.method_names().collect();
    assert_eq!(tested, spec_methods, "Test cases don't cover all spec methods");
    assert_eq!(tested, module_methods, "Test cases don't cover all module methods");

    // Resolve spec method index and validate sample values against spec schemas.
    sample_values
        .into_iter()
        .map(|(name, values)| {
            let method_index =
                methods.iter().position(|m| m["name"].as_str().unwrap() == name).unwrap();
            let spec_params = methods[method_index]["params"].as_array().unwrap();
            assert_eq!(
                spec_params.len(),
                values.len(),
                "Sample value count for {name} doesn't match spec param count"
            );
            for (param_index, (spec_param, sample_value)) in
                spec_params.iter().zip(&values).enumerate()
            {
                let param_name = spec_param["name"].as_str().unwrap();
                let schema = compile_schema_for_ref(
                    &SPEC,
                    &format!("methods/{method_index}/params/{param_index}/schema"),
                );
                assert_matches_schema(
                    &schema,
                    sample_value,
                    &format!("Parameter '{param_name}' of {name}"),
                );
            }
            (name, method_index, values)
        })
        .collect()
}

/// Sends a JSON-RPC request and asserts the module does not reject it with "invalid params".
/// `params_str` is the raw JSON fragment for the `"params"` field (may be empty).
async fn assert_rpc_does_not_reject_params(
    module: &RpcModule<ProvingRpcServerImpl>,
    method_name: &str,
    params_str: &str,
) {
    let req = format!(r#"{{"jsonrpc":"2.0","id":"1","method":"{method_name}"{params_str}}}"#);

    let (resp, _) = module.raw_json_request(&req, 1).await.unwrap();
    let json_resp: Value = serde_json::from_str(resp.get()).unwrap();

    if let Some(error) = json_resp.get("error") {
        let code = error["code"].as_i64().unwrap();
        assert_ne!(
            code,
            i64::from(INVALID_PARAMS_CODE),
            "Method {method_name}: RPC module rejected request with 'invalid params', meaning the \
             spec's parameter names or types don't match the implementation.\nRequest: \
             {req}\nError: {}",
            serde_json::to_string_pretty(error).unwrap(),
        );
    }
}

/// Sends a named-param JSON-RPC request (e.g. `"params": {"block_id": ...}`) and asserts
/// the module accepts it.
async fn assert_rpc_accepts_spec_param_names(
    module: &RpcModule<ProvingRpcServerImpl>,
    method_name: &str,
    method_index: usize,
    sample_values: &[Value],
) {
    let spec_params = SPEC["methods"][method_index]["params"].as_array().unwrap();
    let params_obj: serde_json::Map<String, Value> = spec_params
        .iter()
        .zip(sample_values)
        .map(|(spec_param, value)| {
            (spec_param["name"].as_str().unwrap().to_string(), value.clone())
        })
        .collect();
    let params_str = if params_obj.is_empty() {
        String::new()
    } else {
        format!(r#", "params": {}"#, serde_json::to_string(&params_obj).unwrap())
    };
    assert_rpc_does_not_reject_params(module, method_name, &params_str).await;
}

/// Sends a positional-param JSON-RPC request (e.g. `"params": [...]`) and asserts the module
/// accepts it.
async fn assert_rpc_accepts_spec_param_indices(
    module: &RpcModule<ProvingRpcServerImpl>,
    method_name: &str,
    sample_values: &[Value],
) {
    let params_str = if sample_values.is_empty() {
        String::new()
    } else {
        format!(r#", "params": {}"#, serde_json::to_string(&sample_values).unwrap())
    };
    assert_rpc_does_not_reject_params(module, method_name, &params_str).await;
}

/// Validates that the spec's parameter definitions match the actual RPC implementation.
///
/// For each method, sends both a named-param and a positional-param JSON-RPC request and asserts
/// no "invalid params" error.
#[rstest]
#[tokio::test]
async fn spec_request_params_match_rpc_module(rpc_module: RpcModule<ProvingRpcServerImpl>) {
    let test_cases = build_method_test_cases(&rpc_module);

    for (method_name, method_index, sample_values) in &test_cases {
        assert_rpc_accepts_spec_param_names(&rpc_module, method_name, *method_index, sample_values)
            .await;
        assert_rpc_accepts_spec_param_indices(&rpc_module, method_name, sample_values).await;
    }
}

/// Validates that response types match the spec's result schemas.
///
/// For specVersion: calls the real RPC module and validates the actual response.
/// For proveTransaction: validates a sample result against the spec schema.
#[rstest]
#[tokio::test]
async fn spec_response_schemas_match_rpc_module(rpc_module: RpcModule<ProvingRpcServerImpl>) {
    let methods = SPEC["methods"].as_array().unwrap();

    let req = format!(r#"{{"jsonrpc":"2.0","id":"1","method":"{}"}}"#, SPEC_VERSION_METHOD);
    let (resp, _) = rpc_module.raw_json_request(&req, 1).await.unwrap();
    let spec_version_response: Value = serde_json::from_str(resp.get()).unwrap();

    let method_results: Vec<(&str, Value)> = vec![
        (SPEC_VERSION_METHOD, spec_version_response["result"].clone()),
        (
            PROVE_TRANSACTION_METHOD,
            serde_json::to_value(sample_prove_transaction_result()).unwrap(),
        ),
    ];

    for (method_name, result_value) in &method_results {
        let method_index =
            methods.iter().position(|m| m["name"].as_str().unwrap() == *method_name).unwrap();

        let schema =
            compile_schema_for_ref(&SPEC, &format!("methods/{method_index}/result/schema"));
        assert_matches_schema(&schema, result_value, &format!("{method_name} response"));
    }
}

#[test]
fn error_responses_match_spec() {
    let spec_variants = RpcError::all_spec_variants();

    // Verify we cover every error in the spec.
    let spec_error_keys: Vec<&str> =
        SPEC["components"]["errors"].as_object().unwrap().keys().map(|k| k.as_str()).collect();
    let tested_error_keys: Vec<&str> = spec_variants.iter().map(|(key, _)| *key).collect();
    assert_eq!(
        tested_error_keys.len(),
        spec_error_keys.len(),
        "RpcError::all_spec_variants() doesn't cover all spec errors. Spec has: \
         {spec_error_keys:?}, test has: {tested_error_keys:?}"
    );

    for (spec_key, rpc_error) in spec_variants {
        let actual: ErrorObjectOwned = rpc_error.into();
        SpecError::from_spec(&SPEC["components"]["errors"][spec_key]).assert_matches(&actual);
    }
}
