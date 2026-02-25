use std::collections::HashSet;
use std::sync::LazyLock;

use blockifier_reexecution::state_reader::rpc_objects::BlockId;
use jsonrpsee::server::RpcModule;
use jsonrpsee::types::ErrorObjectOwned;
use jsonschema::JSONSchema;
use rstest::rstest;
use serde_json::Value;
use starknet_api::block::{BlockHash, BlockNumber};
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

/// Validates that the spec's parameter definitions match the actual RPC implementation.
///
/// For each method, this test:
/// 1. Asserts that the test cases cover all spec and module methods (completeness guard).
/// 2. Verifies param names in the spec match what the RPC module expects (by sending a named-param
///    JSON-RPC request and asserting no "invalid params" error).
/// 3. Validates serialized sample values against each param's schema in the spec.
#[tokio::test]
async fn spec_request_params_match_rpc_module() {
    let module = build_test_rpc_module();
    let methods = SPEC["methods"].as_array().unwrap();

    // For each method: (spec_name, sample params as (name, value) pairs).
    // The names MUST match the spec's param names — the test verifies this.
    let method_test_cases: Vec<(&str, Vec<(&str, Value)>)> = vec![
        (SPEC_VERSION_METHOD, vec![]),
        (
            PROVE_TRANSACTION_METHOD,
            vec![
                ("block_id", serde_json::to_value(BlockId::Latest).unwrap()),
                ("transaction", serde_json::to_value(rpc_invoke_tx(invoke_tx_args!())).unwrap()),
            ],
        ),
    ];

    // Completeness guard: test cases must cover all spec and module methods.
    let tested_methods: HashSet<&str> = method_test_cases.iter().map(|(name, _)| *name).collect();
    let spec_methods: HashSet<&str> = methods.iter().map(|m| m["name"].as_str().unwrap()).collect();
    let module_methods: HashSet<&str> = module.method_names().collect();
    assert_eq!(tested_methods, spec_methods, "Test cases don't cover all spec methods");
    assert_eq!(tested_methods, module_methods, "Test cases don't cover all module methods");

    for (method_name, sample_params) in &method_test_cases {
        let spec_method =
            methods.iter().find(|m| m["name"].as_str().unwrap() == *method_name).unwrap();
        let method_index =
            methods.iter().position(|m| m["name"].as_str().unwrap() == *method_name).unwrap();
        let spec_params = spec_method["params"].as_array().unwrap();

        // Verify param count and names match spec.
        let spec_param_names: Vec<&str> =
            spec_params.iter().map(|p| p["name"].as_str().unwrap()).collect();
        let sample_param_names: Vec<&str> = sample_params.iter().map(|(name, _)| *name).collect();
        assert_eq!(
            spec_param_names, sample_param_names,
            "Parameter names for {method_name} don't match spec"
        );

        // Validate each sample value against its spec param schema.
        for (param_index, (param_name, sample_value)) in sample_params.iter().enumerate() {
            let schema = compile_schema_for_ref(
                &SPEC,
                &format!("methods/{method_index}/params/{param_index}/schema"),
            );
            assert_matches_schema(
                &schema,
                sample_value,
                &format!("Parameter '{param_name}' of {method_name}"),
            );
        }

        // Send a named-param JSON-RPC request to the actual RPC module.
        // If the spec's param names or types don't match the implementation,
        // jsonrpsee returns -32602 ("invalid params").
        let params_obj: serde_json::Map<String, Value> =
            sample_params.iter().map(|(name, value)| (name.to_string(), value.clone())).collect();
        let params_str = if params_obj.is_empty() {
            String::new()
        } else {
            format!(r#", "params": {}"#, serde_json::to_string(&params_obj).unwrap())
        };
        let req = format!(r#"{{"jsonrpc":"2.0","id":"1","method":"{method_name}"{params_str}}}"#);

        let (resp, _) = module.raw_json_request(&req, 1).await.unwrap();
        let json_resp: Value = serde_json::from_str(resp.get()).unwrap();

        if let Some(error) = json_resp.get("error") {
            let code = error["code"].as_i64().unwrap();
            assert_ne!(
                code,
                -32602,
                "Method {method_name}: RPC module rejected request with 'invalid params', meaning \
                 the spec's parameter names or types don't match the implementation.\nRequest: \
                 {req}\nError: {}",
                serde_json::to_string_pretty(error).unwrap(),
            );
        }
    }
}

/// Validates that response types match the spec's result schemas.
///
/// For specVersion: calls the real RPC module and validates the actual response.
/// For proveTransaction: validates a sample result against the spec schema.
#[tokio::test]
async fn spec_response_schemas_match_rpc_module() {
    let module = build_test_rpc_module();
    let methods = SPEC["methods"].as_array().unwrap();

    let spec_version_response = raw_call(&module, SPEC_VERSION_METHOD).await;

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

#[rstest]
#[case("BLOCK_NOT_FOUND", error::block_not_found())]
#[case("ACCOUNT_VALIDATION_FAILED", error::validation_failure("test".to_string()))]
#[case("UNSUPPORTED_TX_VERSION", error::unsupported_tx_version("v99".to_string()))]
#[case("SERVICE_BUSY", error::service_busy(2))]
fn error_responses_match_spec(#[case] spec_key: &str, #[case] actual: ErrorObjectOwned) {
    SpecError::from_spec(&SPEC["components"]["errors"][spec_key]).assert_matches(&actual);
}

#[rstest]
#[case("hash", BlockId::Hash(BlockHash(felt!("0x123"))))]
#[case("number", BlockId::Number(BlockNumber(42)))]
#[case("latest", BlockId::Latest)]
fn serialized_block_id_matches_schema(#[case] label: &str, #[case] block_id: BlockId) {
    let schema = compile_schema_for_ref(&SPEC, &format!("components/schemas/BLOCK_ID"));
    assert_matches_schema(&schema, &serde_json::to_value(block_id).unwrap(), label);
}
