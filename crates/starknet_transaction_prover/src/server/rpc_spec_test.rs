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
use crate::server::error;
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

struct MethodTestCase {
    method_name: &'static str,
    method_index: usize,
    sample_params: Vec<Value>,
    sample_result: Value,
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

/// Builds test cases with resolved spec data for each RPC method, including both sample parameters
/// and sample results.
/// Also asserts that the test cases cover every method in the spec and module (completeness guard).
///
/// For specVersion: obtains the result by calling the real RPC module.
/// For proveTransaction: uses a hand-crafted sample result.
async fn build_method_test_cases(module: &RpcModule<ProvingRpcServerImpl>) -> Vec<MethodTestCase> {
    let request = format!(r#"{{"jsonrpc":"2.0","id":"1","method":"{SPEC_VERSION_METHOD}"}}"#);
    let (response, _) = module.raw_json_request(&request, 1).await.unwrap();
    let spec_version_response: Value = serde_json::from_str(response.get()).unwrap();

    let sample_data: Vec<(&str, Vec<Value>, Value)> = vec![
        (SPEC_VERSION_METHOD, vec![], spec_version_response["result"].clone()),
        (
            PROVE_TRANSACTION_METHOD,
            vec![
                serde_json::to_value(BlockId::Latest).unwrap(),
                serde_json::to_value(rpc_invoke_tx(invoke_tx_args!())).unwrap(),
            ],
            serde_json::to_value(sample_prove_transaction_result()).unwrap(),
        ),
    ];

    // Completeness guard: test cases must cover all spec and module methods.
    let methods = SPEC["methods"].as_array().unwrap();
    let tested: HashSet<&str> = sample_data.iter().map(|(name, _, _)| *name).collect();
    let spec_methods: HashSet<&str> = methods.iter().map(|m| m["name"].as_str().unwrap()).collect();
    let module_methods: HashSet<&str> = module.method_names().collect();
    assert_eq!(tested, spec_methods, "Test cases don't cover all spec methods");
    assert_eq!(tested, module_methods, "Test cases don't cover all module methods");

    // Resolve spec method index and validate sample params against spec schemas.
    sample_data
        .into_iter()
        .map(|(name, params, result)| {
            let method_index =
                methods.iter().position(|m| m["name"].as_str().unwrap() == name).unwrap();
            let spec_params = methods[method_index]["params"].as_array().unwrap();
            assert_eq!(
                spec_params.len(),
                params.len(),
                "Sample param count for {name} doesn't match spec param count"
            );
            for (param_index, (spec_param, sample_value)) in
                spec_params.iter().zip(&params).enumerate()
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
            MethodTestCase {
                method_name: name,
                method_index,
                sample_params: params,
                sample_result: result,
            }
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
        .map(|(spec_param, sample_value)| {
            (spec_param["name"].as_str().unwrap().to_string(), sample_value.clone())
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

/// Validates that the spec's parameter and result definitions match the actual RPC implementation.
///
/// For each method, validates request params (both named and positional) and response schemas.
#[rstest]
#[tokio::test]
async fn test_spec_matches_rpc_module(rpc_module: RpcModule<ProvingRpcServerImpl>) {
    let test_cases = build_method_test_cases(&rpc_module).await;

    for MethodTestCase { method_name, method_index, sample_params, sample_result } in &test_cases {
        assert_rpc_accepts_spec_param_names(&rpc_module, method_name, *method_index, sample_params)
            .await;
        assert_rpc_accepts_spec_param_indices(&rpc_module, method_name, sample_params).await;

        let schema =
            compile_schema_for_ref(&SPEC, &format!("methods/{method_index}/result/schema"));
        assert_matches_schema(&schema, sample_result, &format!("{method_name} response"));
    }
}

#[rstest]
#[tokio::test]
async fn test_prove_transaction_rejects_pending_block_id(
    rpc_module: RpcModule<ProvingRpcServerImpl>,
) {
    let params = serde_json::to_string(&vec![
        serde_json::to_value(BlockId::Pending).unwrap(),
        serde_json::to_value(rpc_invoke_tx(invoke_tx_args!())).unwrap(),
    ])
    .unwrap();
    let request = format!(
        r#"{{"jsonrpc":"2.0","id":"1","method":"{PROVE_TRANSACTION_METHOD}", "params": {params}}}"#
    );

    let (response, _) = rpc_module.raw_json_request(&request, 1).await.unwrap();
    let json_response: Value = serde_json::from_str(response.get()).unwrap();
    let error_value = json_response
        .get("error")
        .unwrap_or_else(|| panic!("Expected error response for pending block id. Got: {json_response}"));
    let actual_error: ErrorObjectOwned = serde_json::from_value(error_value.clone()).unwrap();

    SpecError::from_spec(&SPEC["components"]["errors"]["BLOCK_NOT_FOUND"])
        .assert_matches(&actual_error);
}

#[test]
// TODO(Avi): Add an error enum to make this test exhastive.
fn test_error_responses_match_spec() {
    let test_cases: Vec<(&str, ErrorObjectOwned)> = vec![
        ("BLOCK_NOT_FOUND", error::block_not_found()),
        ("ACCOUNT_VALIDATION_FAILED", error::validation_failure("test".to_string())),
        ("UNSUPPORTED_TX_VERSION", error::unsupported_tx_version("v99".to_string())),
        ("SERVICE_BUSY", error::service_busy(2)),
    ];

    // Completeness guard: ensure all spec errors have a test case.
    let spec_error_keys: HashSet<&str> =
        SPEC["components"]["errors"].as_object().unwrap().keys().map(|k| k.as_str()).collect();
    let tested_error_keys: HashSet<&str> = test_cases.iter().map(|(key, _)| *key).collect();
    assert_eq!(
        tested_error_keys, spec_error_keys,
        "Test cases don't cover all spec errors. Update the test_cases list above."
    );

    for (spec_key, actual) in &test_cases {
        SpecError::from_spec(&SPEC["components"]["errors"][spec_key]).assert_matches(actual);
    }
}
