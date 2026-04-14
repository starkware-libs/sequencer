use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, LazyLock};

use blockifier_reexecution::state_reader::rpc_objects::BlockId;
use jsonrpsee::server::RpcModule;
use jsonrpsee::types::error::INVALID_PARAMS_CODE;
use jsonrpsee::types::ErrorObjectOwned;
use jsonschema::JSONSchema;
use rstest::{fixture, rstest};
use serde_json::Value;
use starknet_api::block::GasPrice;
use starknet_api::execution_resources::GasAmount;
use starknet_api::invoke_tx_args;
use starknet_api::test_utils::invoke::rpc_invoke_tx;
use starknet_api::transaction::fields::{
    AllResourceBounds,
    Proof,
    ProofFacts,
    ResourceBounds,
    ValidResourceBounds,
};
use starknet_types_core::felt::Felt;

use crate::config::ProverConfig;
use crate::proving::virtual_snos_prover::RpcVirtualSnosProver;
use crate::server::errors;
use crate::server::mock_rpc::MockProvingRpc;
use crate::server::rpc_api::ProvingRpcServer;
use crate::server::rpc_impl::ProvingRpcServerImpl;

const SPEC_VERSION_METHOD: &str = "starknet_specVersion";
const PROVE_TRANSACTION_METHOD: &str = "starknet_proveTransaction";
const DUMMY_RPC_NODE_URL: &str = "http://localhost:1";
const TEST_MAX_CONCURRENT_REQUESTS: usize = 2;
const RPC_RESPONSE_BUFFER_SIZE: usize = 1;

const STARKNET_SPECS_REPO: &str = "https://github.com/starkware-libs/starknet-specs.git";

/// Pinned revision of starknet-specs. Update this when the spec changes.
// TODO(Avi): Update to a main-branch commit once the proving spec is merged to main.
const STARKNET_SPECS_REV: &str =
    include_str!("../../resources/starknet_specs_rev.txt").trim_ascii();

/// Returns the path to the starknet-specs directory.
///
/// If `STARKNET_SPECS_DIR` is set, uses that directory directly (for CI environments that
/// pre-clone the repo). Otherwise, clones to a deterministic cache path keyed by revision,
/// so repeated local runs reuse the same checkout.
static SPECS_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    if let Ok(dir) = std::env::var("STARKNET_SPECS_DIR") {
        let path = PathBuf::from(dir);
        assert!(path.join("api").exists(), "STARKNET_SPECS_DIR does not contain an api/ directory");
        assert!(
            path.join("proving-api").exists(),
            "STARKNET_SPECS_DIR does not contain a proving-api/ directory"
        );
        return path;
    }

    let cache_path = std::env::temp_dir().join(format!("starknet-specs-{STARKNET_SPECS_REV}"));

    // Skip cloning if the cached checkout already exists.
    if cache_path.join("api").exists() && cache_path.join("proving-api").exists() {
        return cache_path;
    }

    let dir_str = cache_path.to_str().unwrap();

    // Clone then checkout the pinned revision.
    let output = std::process::Command::new("git")
        .args(["clone", "--filter=blob:none", "--sparse", STARKNET_SPECS_REPO, dir_str])
        .output()
        .expect("Failed to run git clone for starknet-specs");
    assert!(
        output.status.success(),
        "Failed to clone starknet-specs.\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let output = std::process::Command::new("git")
        .args(["-C", dir_str, "checkout", STARKNET_SPECS_REV])
        .output()
        .expect("Failed to run git checkout for starknet-specs");
    assert!(
        output.status.success(),
        "Failed to checkout starknet-specs revision {STARKNET_SPECS_REV}.\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Sparse-checkout only the directories containing the spec JSON files.
    let output = std::process::Command::new("git")
        .args(["-C", dir_str, "sparse-checkout", "set", "api", "proving-api"])
        .output()
        .expect("Failed to run git sparse-checkout for starknet-specs");
    assert!(
        output.status.success(),
        "Failed to set sparse-checkout for starknet-specs.\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    cache_path
});

fn specs_path() -> PathBuf {
    SPECS_DIR.clone()
}

fn read_spec_file(path: &str) -> Value {
    let full_path = specs_path().join(path);
    let content = std::fs::read_to_string(&full_path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {e}", full_path.display()));
    serde_json::from_str(&content)
        .unwrap_or_else(|e| panic!("Failed to parse {}: {e}", full_path.display()))
}

static SPEC: LazyLock<Value> =
    LazyLock::new(|| read_spec_file("proving-api/starknet_proving_api_openrpc.json"));

static MAIN_API_SPEC: LazyLock<Value> =
    LazyLock::new(|| read_spec_file("api/starknet_api_openrpc.json"));

static WRITE_API_SPEC: LazyLock<Value> =
    LazyLock::new(|| read_spec_file("api/starknet_write_api.json"));

/// Pre-resolved map of all error names to their definitions, collected from method error arrays.
/// Handles both local `#/components/errors/...` refs and external `./file.json#/...` refs.
static SPEC_ERRORS: LazyLock<BTreeMap<String, Value>> = LazyLock::new(|| {
    let mut errors = BTreeMap::new();
    for method in SPEC["methods"].as_array().unwrap() {
        let Some(error_refs) = method.get("errors").and_then(|e| e.as_array()) else {
            continue;
        };
        for error_ref in error_refs {
            let ref_str = error_ref["$ref"].as_str().unwrap();
            let error_name = ref_str.rsplit('/').next().unwrap().to_string();
            if errors.contains_key(&error_name) {
                continue;
            }
            let resolved = if ref_str.starts_with('#') {
                let path = ref_str.trim_start_matches("#/");
                path.split('/').fold((*SPEC).clone(), |value, key| value[key].clone())
            } else {
                resolve_ref(ref_str)
            };
            errors.insert(error_name, resolved);
        }
    }
    errors
});

#[fixture]
fn rpc_module() -> RpcModule<ProvingRpcServerImpl> {
    let config =
        ProverConfig { rpc_node_url: DUMMY_RPC_NODE_URL.to_string(), ..Default::default() };
    let rpc_impl =
        ProvingRpcServerImpl::new(RpcVirtualSnosProver::new(&config), TEST_MAX_CONCURRENT_REQUESTS);
    rpc_impl.into_rpc()
}

#[fixture]
fn mock_rpc_module() -> RpcModule<MockProvingRpc> {
    MockProvingRpc::from_expected_json().into_rpc()
}

/// Compiles a JSON Schema from a `$ref` path within the spec document.
///
/// Registers external spec documents so that `$ref`s to other files
/// (e.g. `../api/starknet_api_openrpc.json#/...`) can be resolved.
fn compile_schema_for_ref(spec: &Value, ref_path: &str) -> JSONSchema {
    let ref_uri = format!("file:///spec#/{ref_path}");
    let ref_schema: Value = serde_json::from_str(&format!(r#"{{"$ref": "{ref_uri}"}}"#)).unwrap();

    JSONSchema::options()
        .with_document("file:///spec".to_string(), spec.clone())
        .with_document("file:///api/starknet_api_openrpc.json".to_string(), MAIN_API_SPEC.clone())
        .with_document("file:///api/starknet_write_api.json".to_string(), WRITE_API_SPEC.clone())
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
        assert_eq!(
            error.data().is_some(),
            self.expects_data,
            "Mismatch on `data` field: spec {} it but implementation {}",
            if self.expects_data { "includes" } else { "omits" },
            if error.data().is_some() { "includes it" } else { "omits it" },
        );
    }
}

/// Resolves an external `$ref` by selecting the referenced spec document
/// and walking the JSON pointer to the referenced object.
///
/// Example:
/// `../api/starknet_api_openrpc.json#/components/errors/BLOCK_NOT_FOUND`
/// resolves to the `BLOCK_NOT_FOUND` error object inside that spec.
fn resolve_ref(ref_str: &str) -> Value {
    let (file_part, json_pointer) = ref_str.split_once('#').expect("Invalid $ref: missing '#'");

    let spec_doc = match file_part {
        f if f.ends_with("starknet_api_openrpc.json") => &*MAIN_API_SPEC,
        f if f.ends_with("starknet_write_api.json") => &*WRITE_API_SPEC,
        _ => panic!("Unknown external ref: {ref_str}"),
    };

    let mut referenced_value = spec_doc.clone();
    for key in json_pointer.trim_start_matches('/').split('/') {
        referenced_value = referenced_value
            .get(key)
            .unwrap_or_else(|| panic!("Invalid $ref path '{json_pointer}' at '{key}'"))
            .clone();
    }

    referenced_value
}

/// Resolves a spec error by name, looking up the pre-resolved error map.
fn resolve_spec_error(error_key: &str) -> Value {
    SPEC_ERRORS
        .get(error_key)
        .unwrap_or_else(|| panic!("Error '{error_key}' not found in spec method error arrays"))
        .clone()
}

/// Builds test cases for each RPC method with sample parameters.
/// Also asserts that the test cases cover every method in the spec and module (completeness guard).
///
/// The test cases only define the requests (method name + params). The response is obtained by
/// calling the mock RPC module in the test function itself.
fn build_method_test_cases(module: &RpcModule<MockProvingRpc>) -> Vec<MethodTestCase> {
    let sample_data: Vec<(&str, Vec<Value>)> = vec![
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
    let tested_methods: HashSet<&str> = sample_data.iter().map(|(name, _)| *name).collect();
    let spec_methods: HashSet<&str> = methods.iter().map(|m| m["name"].as_str().unwrap()).collect();
    let module_methods: HashSet<&str> = module.method_names().collect();
    assert_eq!(tested_methods, spec_methods, "Test cases don't cover all spec methods");
    assert_eq!(tested_methods, module_methods, "Test cases don't cover all module methods");

    // Resolve spec method index and validate sample params against spec schemas.
    sample_data
        .into_iter()
        .map(|(name, params)| {
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
            MethodTestCase { method_name: name, method_index, sample_params: params }
        })
        .collect()
}

fn format_params(sample_values: &[Value]) -> String {
    if sample_values.is_empty() {
        String::new()
    } else {
        format!(r#","params":{}"#, serde_json::to_string(sample_values).unwrap())
    }
}

fn format_rpc_request(method_name: &str, params_fragment: &str) -> String {
    format!(r#"{{"jsonrpc":"2.0","id":"1","method":"{method_name}"{params_fragment}}}"#)
}

/// Sends a JSON-RPC request and asserts the module does not reject it with "invalid params".
/// `params_str` is the raw JSON fragment for the `"params"` field (may be empty).
async fn assert_rpc_does_not_reject_params(
    module: &RpcModule<MockProvingRpc>,
    method_name: &str,
    params_str: &str,
) {
    let req = format_rpc_request(method_name, params_str);

    let (resp, _) = module.raw_json_request(&req, RPC_RESPONSE_BUFFER_SIZE).await.unwrap();
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
    module: &RpcModule<MockProvingRpc>,
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
        format!(r#","params":{}"#, serde_json::to_string(&params_obj).unwrap())
    };
    assert_rpc_does_not_reject_params(module, method_name, &params_str).await;
}

/// Sends a positional-param JSON-RPC request (e.g. `"params": [...]`) and asserts the module
/// accepts it.
async fn assert_rpc_accepts_spec_param_indices(
    module: &RpcModule<MockProvingRpc>,
    method_name: &str,
    sample_values: &[Value],
) {
    let params_str = format_params(sample_values);
    assert_rpc_does_not_reject_params(module, method_name, &params_str).await;
}

/// Validates that the spec's parameter and result definitions match the actual RPC implementation.
///
/// For each method:
/// 1. Validates request params (both named and positional) are accepted by the module.
/// 2. Calls the mock RPC to obtain the response, then validates it against the spec schema.
#[rstest]
#[tokio::test]
async fn test_spec_matches_rpc_module(mock_rpc_module: RpcModule<MockProvingRpc>) {
    let test_cases = build_method_test_cases(&mock_rpc_module);

    for MethodTestCase { method_name, method_index, sample_params } in &test_cases {
        assert_rpc_accepts_spec_param_names(
            &mock_rpc_module,
            method_name,
            *method_index,
            sample_params,
        )
        .await;
        assert_rpc_accepts_spec_param_indices(&mock_rpc_module, method_name, sample_params).await;

        // Call the mock RPC to obtain the response.
        let params_str = format_params(sample_params);
        let request = format_rpc_request(method_name, &params_str);
        let (resp, _) =
            mock_rpc_module.raw_json_request(&request, RPC_RESPONSE_BUFFER_SIZE).await.unwrap();
        let json_resp: Value = serde_json::from_str(resp.get()).unwrap();
        let result = json_resp.get("result").unwrap_or_else(|| {
            panic!("Expected successful response for {method_name}, got: {json_resp}")
        });

        let schema =
            compile_schema_for_ref(&SPEC, &format!("methods/{method_index}/result/schema"));
        assert_matches_schema(&schema, result, &format!("{method_name} response"));
    }
}

#[rstest]
#[tokio::test]
async fn test_prove_transaction_rejects_pending_block_id(
    rpc_module: RpcModule<ProvingRpcServerImpl>,
) {
    let sample_values = vec![
        serde_json::to_value(BlockId::Pending).unwrap(),
        serde_json::to_value(rpc_invoke_tx(invoke_tx_args!())).unwrap(),
    ];
    let params_str = format_params(&sample_values);
    let request = format_rpc_request(PROVE_TRANSACTION_METHOD, &params_str);

    let (response, _) =
        rpc_module.raw_json_request(&request, RPC_RESPONSE_BUFFER_SIZE).await.unwrap();
    let json_response: Value = serde_json::from_str(response.get()).unwrap();
    let error_value = json_response.get("error").unwrap_or_else(|| {
        panic!("Expected error response for pending block id. Got: {json_response}")
    });
    let actual_error: ErrorObjectOwned = serde_json::from_value(error_value.clone()).unwrap();

    SpecError::from_spec(&resolve_spec_error("BLOCK_NOT_FOUND")).assert_matches(&actual_error);
}

#[test]
// TODO(Avi): Add an error enum to make this test exhastive.
fn test_error_responses_match_spec() {
    let test_cases: Vec<(&str, ErrorObjectOwned)> = vec![
        ("BLOCK_NOT_FOUND", errors::block_not_found()),
        ("ACCOUNT_VALIDATION_FAILED", errors::validation_failure("test".to_string())),
        ("UNSUPPORTED_TX_VERSION", errors::unsupported_tx_version("v99".to_string())),
        ("SERVICE_BUSY", errors::service_busy(2)),
        (
            "INVALID_TRANSACTION_INPUT",
            errors::invalid_transaction_input("test field invalid".to_string()),
        ),
    ];

    // Completeness guard: ensure all spec errors (from method error arrays) have a test case.
    let spec_error_keys: HashSet<&str> = SPEC_ERRORS.keys().map(|k| k.as_str()).collect();
    let tested_error_keys: HashSet<&str> = test_cases.iter().map(|(key, _)| *key).collect();
    assert_eq!(
        tested_error_keys, spec_error_keys,
        "Test cases don't cover all spec errors. Update the test_cases list above."
    );

    for (spec_key, actual) in &test_cases {
        SpecError::from_spec(&resolve_spec_error(spec_key)).assert_matches(actual);
    }
}

/// Helper: sends a prove_transaction request and asserts it returns the expected error.
async fn assert_prove_transaction_error(
    rpc_module: &RpcModule<ProvingRpcServerImpl>,
    transaction: starknet_api::rpc_transaction::RpcTransaction,
    expected_spec_error_key: &str,
) {
    let sample_values = vec![
        serde_json::to_value(BlockId::Latest).unwrap(),
        serde_json::to_value(transaction).unwrap(),
    ];
    let params_str = format_params(&sample_values);
    let request = format_rpc_request(PROVE_TRANSACTION_METHOD, &params_str);

    let (response, _) =
        rpc_module.raw_json_request(&request, RPC_RESPONSE_BUFFER_SIZE).await.unwrap();
    let json_response: Value = serde_json::from_str(response.get()).unwrap();
    let error_value = json_response.get("error").unwrap_or_else(|| {
        panic!("Expected error response for {expected_spec_error_key}. Got: {json_response}")
    });
    let actual_error: ErrorObjectOwned = serde_json::from_value(error_value.clone()).unwrap();

    SpecError::from_spec(&resolve_spec_error(expected_spec_error_key))
        .assert_matches(&actual_error);
}

#[rstest]
#[tokio::test]
async fn test_prove_transaction_rejects_non_empty_proof(
    rpc_module: RpcModule<ProvingRpcServerImpl>,
) {
    let transaction = rpc_invoke_tx(invoke_tx_args!(
        resource_bounds: ValidResourceBounds::AllResources(AllResourceBounds::default()),
        proof: Proof::from(vec![1u8])
    ));
    assert_prove_transaction_error(&rpc_module, transaction, "INVALID_TRANSACTION_INPUT").await;
}

#[rstest]
#[tokio::test]
async fn test_prove_transaction_rejects_non_empty_proof_facts(
    rpc_module: RpcModule<ProvingRpcServerImpl>,
) {
    let transaction = rpc_invoke_tx(invoke_tx_args!(
        resource_bounds: ValidResourceBounds::AllResources(AllResourceBounds::default()),
        proof_facts: ProofFacts(Arc::new(vec![Felt::ONE]))
    ));
    assert_prove_transaction_error(&rpc_module, transaction, "INVALID_TRANSACTION_INPUT").await;
}

#[rstest]
#[tokio::test]
async fn test_prove_transaction_rejects_non_zero_fee(rpc_module: RpcModule<ProvingRpcServerImpl>) {
    // Non-zero gas amount * non-zero gas price → non-zero max possible fee.
    let non_zero_fee_bounds = AllResourceBounds {
        l2_gas: ResourceBounds { max_amount: GasAmount(1), max_price_per_unit: GasPrice(1) },
        ..Default::default()
    };
    let transaction = rpc_invoke_tx(invoke_tx_args!(
        resource_bounds: ValidResourceBounds::AllResources(non_zero_fee_bounds)
    ));
    assert_prove_transaction_error(&rpc_module, transaction, "INVALID_TRANSACTION_INPUT").await;
}
