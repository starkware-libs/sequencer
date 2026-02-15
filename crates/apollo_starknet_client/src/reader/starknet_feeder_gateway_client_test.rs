use std::collections::HashMap;
use std::fmt::Debug;
use std::future::Future;

use assert_matches::assert_matches;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use indexmap::indexmap;
use mockito::ServerGuard;
use pretty_assertions::assert_eq;
use serde_json::Value;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::contract_class::EntryPointType;
use starknet_api::core::{ContractAddress, EntryPointSelector, GlobalRoot, SequencerPublicKey};
use starknet_api::crypto::utils::PublicKey;
use starknet_api::deprecated_contract_class::{
    ConstructorType,
    ContractClass as DeprecatedContractClass,
    ContractClassAbiEntry,
    EntryPointOffset,
    EntryPointV0 as DeprecatedEntryPoint,
    FunctionAbiEntry,
    Program,
    TypedParameter,
};
use starknet_api::state::{EntryPoint, FunctionIndex};
use starknet_api::transaction::fields::{Fee, TransactionSignature};
use starknet_api::transaction::{TransactionHash, TransactionVersion};
use starknet_api::{class_hash, contract_address, felt, nonce};

use super::objects::state::StateUpdate;
use super::objects::transaction::IntermediateDeclareTransaction;
use super::{
    ContractClass,
    GenericContractClass,
    PendingData,
    ReaderClientError,
    ReaderClientResult,
    StarknetFeederGatewayClient,
    StarknetReader,
    BLOCK_NUMBER_QUERY,
    CLASS_HASH_QUERY,
    GET_BLOCK_URL,
    GET_STATE_UPDATE_URL,
};
use crate::reader::objects::block::{BlockSignatureData, BlockSignatureMessage};
use crate::reader::Block;
use crate::test_utils::read_resource::read_resource_file;
use crate::test_utils::retry::get_test_config;
use crate::{KnownStarknetErrorCode, StarknetError, StarknetErrorCode};

const NODE_VERSION: &str = "NODE VERSION";
const FEEDER_GATEWAY_ALIVE_RESPONSE: &str = "FeederGateway is alive!";

fn get_block_url(block_number_or_latest: Option<u64>) -> String {
    let url = match block_number_or_latest {
        Some(block_number) => {
            format!("/feeder_gateway/get_block?blockNumber={block_number}&withFeeMarketInfo=true")
        }
        _ => "/feeder_gateway/get_block?blockNumber=latest&withFeeMarketInfo=true".to_string(),
    };
    url
}

fn apollo_starknet_client(server: &ServerGuard) -> StarknetFeederGatewayClient {
    StarknetFeederGatewayClient::new(&server.url(), None, NODE_VERSION, get_test_config(), false)
        .unwrap()
}

fn apollo_starknet_client_with_compression(server: &ServerGuard) -> StarknetFeederGatewayClient {
    StarknetFeederGatewayClient::new(&server.url(), None, NODE_VERSION, get_test_config(), true)
        .unwrap()
}

// TODO(Ayelet): Consider making this function generic for all successful mock responses in this
// file.
async fn mock_successful_get_block_response(
    server: &mut ServerGuard,
    response_file: &str,
    request_param: Option<u64>,
) -> mockito::Mock {
    server
        .mock("GET", get_block_url(request_param).as_str())
        .with_status(200)
        .with_body(read_resource_file(response_file))
        .create_async()
        .await
}

// TODO(Ayelet): Consider making this function generic for all error mock responses in this file.
async fn mock_error_get_block_response(
    server: &mut ServerGuard,
    error_response_body: String,
    request_param: Option<u64>,
) -> mockito::Mock {
    server
        .mock("GET", get_block_url(request_param).as_str())
        .with_status(400)
        .with_body(error_response_body)
        .create_async()
        .await
}

fn block_not_found_error(block_number: i64) -> String {
    let error = StarknetError {
        code: StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::BlockNotFound),
        message: format!("Block {block_number} was not found."),
    };
    serde_json::to_string(&error).unwrap()
}

#[test]
fn new_urls() {
    let url_base_str = "https://url";
    let apollo_starknet_client = StarknetFeederGatewayClient::new(
        url_base_str,
        None,
        NODE_VERSION,
        get_test_config(),
        false,
    )
    .unwrap();
    assert_eq!(
        apollo_starknet_client.urls.get_block.as_str(),
        url_base_str.to_string() + "/" + GET_BLOCK_URL
    );
    assert_eq!(
        apollo_starknet_client.urls.get_state_update.as_str(),
        url_base_str.to_string() + "/" + GET_STATE_UPDATE_URL
    );
}

#[tokio::test]
async fn get_latest_block_when_blocks_exists() {
    let mut server = mockito::Server::new_async().await;
    let apollo_starknet_client = apollo_starknet_client(&server);
    let mock_block =
        mock_successful_get_block_response(&mut server, "reader/block_post_0_14_0.json", None)
            .await;
    let latest_block = apollo_starknet_client.latest_block().await.unwrap();
    mock_block.assert_async().await;
    assert_eq!(latest_block.unwrap().block_number(), BlockNumber(329526));
}

#[tokio::test]
async fn get_latest_block_when_no_blocks_exist() {
    let mut server = mockito::Server::new_async().await;
    let apollo_starknet_client = apollo_starknet_client(&server);
    let mock_no_block =
        mock_error_get_block_response(&mut server, block_not_found_error(-1), None).await;
    let latest_block = apollo_starknet_client.latest_block().await.unwrap();
    mock_no_block.assert_async().await;
    assert!(latest_block.is_none());
}

#[tokio::test]
async fn declare_tx_serde() {
    let declare_tx = IntermediateDeclareTransaction {
        class_hash: class_hash!(
            "0x7319e2f01b0947afd86c0bb0e95029551b32f6dc192c47b2e8b08415eebbc25"
        ),
        compiled_class_hash: None,
        sender_address: contract_address!("0x1"),
        nonce: nonce!(0_u64),
        max_fee: Some(Fee(0)),
        version: TransactionVersion::ONE,
        resource_bounds: None,
        tip: None,
        nonce_data_availability_mode: None,
        fee_data_availability_mode: None,
        paymaster_data: None,
        account_deployment_data: None,
        transaction_hash: TransactionHash(felt!(
            "0x2f2ef64daffdc72bf33b34ad024891691b8eb1d0ab70cc7f8fb71f6fd5e1f22"
        )),
        signature: TransactionSignature::default(),
    };
    let raw_declare_tx = serde_json::to_string(&declare_tx).unwrap();
    assert_eq!(declare_tx, serde_json::from_str(&raw_declare_tx).unwrap());
}

#[tokio::test]
async fn state_update() {
    let mut server = mockito::Server::new_async().await;
    let apollo_starknet_client = apollo_starknet_client(&server);
    let raw_state_update = read_resource_file("reader/block_state_update.json");
    let mock_state_update = server
        .mock("GET", &format!("/feeder_gateway/get_state_update?{BLOCK_NUMBER_QUERY}=123456")[..])
        .with_status(200)
        .with_body(&raw_state_update)
        .create_async()
        .await;
    let state_update = apollo_starknet_client.state_update(BlockNumber(123456)).await.unwrap();
    mock_state_update.assert_async().await;
    let expected_state_update: StateUpdate = serde_json::from_str(&raw_state_update).unwrap();
    assert_eq!(state_update.unwrap(), expected_state_update);

    let mock_no_block = server
        .mock("GET", &format!("/feeder_gateway/get_state_update?{BLOCK_NUMBER_QUERY}=999999")[..])
        .with_status(400)
        .with_body(block_not_found_error(-1))
        .create_async()
        .await;
    let state_update = apollo_starknet_client.state_update(BlockNumber(999999)).await.unwrap();
    assert!(state_update.is_none());
    mock_no_block.assert_async().await;
}

#[tokio::test]
async fn serialization_precision() {
    let input =
        "{\"value\":244116128358498188146337218061232635775543270890529169229936851982759783745}";
    let serialized = serde_json::from_str::<serde_json::Value>(input).unwrap();
    let deserialized = serde_json::to_string(&serialized).unwrap();
    assert_eq!(input, deserialized);
}

#[tokio::test]
async fn contract_class() {
    let mut server = mockito::Server::new_async().await;
    let apollo_starknet_client = apollo_starknet_client(&server);
    let expected_contract_class = ContractClass {
        sierra_program: vec![felt!("0x302e312e30"), felt!("0x1c"), felt!("0x52616e6765436865636b")],
        entry_points_by_type: HashMap::from([
            (
                EntryPointType::External,
                vec![EntryPoint {
                    function_idx: FunctionIndex(0),
                    selector: EntryPointSelector(felt!(
                        "0x22ff5f21f0b81b113e63f7db6da94fedef11b2119b4088b89664fb9a3cb658"
                    )),
                }],
            ),
            (EntryPointType::Constructor, vec![]),
            (EntryPointType::L1Handler, vec![]),
        ]),
        contract_class_version: String::from("0.1.0"),
        abi: String::from(
            "[\n  {\n    \"type\": \"function\",\n    \"name\": \"test\",\n    \"inputs\": [\n      {\n        \"name\": \"arg\",\n        \"ty\": \"core::felt\"\n      },\n      {\n        \"name\": \"arg1\",\n        \"ty\": \"core::felt\"\n      },\n      {\n        \"name\": \"arg2\",\n        \"ty\": \"core::felt\"\n      }\n    ],\n    \"output_ty\": \"core::felt\",\n    \"state_mutability\": \"external\"\n  },\n  {\n    \"type\": \"function\",\n    \"name\": \"empty\",\n    \"inputs\": [],\n    \"output_ty\": \"()\",\n    \"state_mutability\": \"external\"\n  },\n  {\n    \"type\": \"function\",\n    \"name\": \"call_foo\",\n    \"inputs\": [\n      {\n        \"name\": \"a\",\n        \"ty\": \"core::integer::u128\"\n      }\n    ],\n    \"output_ty\": \"core::integer::u128\",\n    \"state_mutability\": \"external\"\n  }\n]",
        ),
    };

    let mock_by_hash = server
        .mock(
            "GET",
            &format!(
            "/feeder_gateway/get_class_by_hash?blockNumber=pending&\
             {CLASS_HASH_QUERY}=0x4e70b19333ae94bd958625f7b61ce9eec631653597e68645e13780061b2136c"
        )[..],
        )
        .with_status(200)
        .with_body(read_resource_file("reader/contract_class.json"))
        .create_async()
        .await;
    let contract_class = apollo_starknet_client
        .class_by_hash(class_hash!(
            "0x4e70b19333ae94bd958625f7b61ce9eec631653597e68645e13780061b2136c"
        ))
        .await
        .unwrap()
        .unwrap();

    let contract_class = match contract_class {
        GenericContractClass::Cairo1ContractClass(class) => class,
        _ => unreachable!("Expecting Cairo0ContractClass."),
    };
    mock_by_hash.assert_async().await;
    assert_eq!(contract_class, expected_contract_class);
}

#[tokio::test]
async fn deprecated_contract_class() {
    let mut server = mockito::Server::new_async().await;
    let apollo_starknet_client = apollo_starknet_client(&server);
    let expected_contract_class = DeprecatedContractClass {
        abi: Some(vec![ContractClassAbiEntry::Constructor(FunctionAbiEntry::<ConstructorType> {
            name: "constructor".to_string(),
            inputs: vec![TypedParameter {
                name: "implementation".to_string(),
                r#type: "felt".to_string(),
            }],
            outputs: vec![],
            state_mutability: None,
            r#type: ConstructorType::Constructor,
        })]),
        program: Program {
            attributes: serde_json::Value::Array(vec![serde_json::json!(1234)]),
            builtins: serde_json::Value::Array(Vec::new()),
            compiler_version: serde_json::Value::Null,
            data: serde_json::Value::Array(vec![
                serde_json::Value::String("0x20780017fff7ffd".to_string()),
                serde_json::Value::String("0x4".to_string()),
                serde_json::Value::String("0x400780017fff7ffd".to_string()),
            ]),
            debug_info: serde_json::Value::Null,
            hints: serde_json::Value::Object(serde_json::Map::new()),
            identifiers: serde_json::Value::Object(serde_json::Map::new()),
            main_scope: serde_json::Value::String("__main__".to_string()),
            prime: serde_json::Value::String(
                "0x800000000000011000000000000000000000000000000000000000000000001".to_string(),
            ),
            reference_manager: serde_json::Value::Object(serde_json::Map::new()),
        },
        entry_points_by_type: HashMap::from([
            (EntryPointType::L1Handler, vec![]),
            (
                EntryPointType::Constructor,
                vec![DeprecatedEntryPoint {
                    selector: EntryPointSelector(felt!(
                        "0x028ffe4ff0f226a9107253e17a904099aa4f63a02a5621de0576e5aa71bc5194"
                    )),
                    offset: EntryPointOffset(62),
                }],
            ),
            (
                EntryPointType::External,
                vec![DeprecatedEntryPoint {
                    selector: EntryPointSelector(felt!(
                        "0x0000000000000000000000000000000000000000000000000000000000000000"
                    )),
                    offset: EntryPointOffset(86),
                }],
            ),
        ]),
    };
    let mock_by_hash = server
        .mock(
            "GET",
            &format!(
            "/feeder_gateway/get_class_by_hash?blockNumber=pending&\
             {CLASS_HASH_QUERY}=0x7af612493193c771c1b12f511a8b4d3b0c6d0648242af4680c7cd0d06186f17"
        )[..],
        )
        .with_status(200)
        .with_body(read_resource_file("reader/deprecated_contract_class.json"))
        .create_async()
        .await;
    let contract_class = apollo_starknet_client
        .class_by_hash(class_hash!(
            "0x7af612493193c771c1b12f511a8b4d3b0c6d0648242af4680c7cd0d06186f17"
        ))
        .await
        .unwrap()
        .unwrap();
    let contract_class = match contract_class {
        GenericContractClass::Cairo0ContractClass(class) => class,
        _ => unreachable!("Expecting deprecated contract class."),
    };
    mock_by_hash.assert_async().await;
    assert_eq!(contract_class, expected_contract_class);

    // Undeclared class.
    let body = r#"{"code": "StarknetErrorCode.UNDECLARED_CLASS", "message": "Class with hash 0x7 is not declared."}"#;
    let mock_by_hash = server
        .mock(
            "GET",
            &format!(
                "/feeder_gateway/get_class_by_hash?blockNumber=pending&{CLASS_HASH_QUERY}=0x7"
            )[..],
        )
        .with_status(400)
        .with_body(body)
        .create_async()
        .await;
    let class = apollo_starknet_client.class_by_hash(class_hash!("0x7")).await.unwrap();
    mock_by_hash.assert_async().await;
    assert!(class.is_none());
}

// TODO(DanB): Add test for pending_data.

#[tokio::test]
async fn deprecated_pending_data() {
    let mut server = mockito::Server::new_async().await;
    let apollo_starknet_client = apollo_starknet_client(&server);

    // Pending
    let raw_pending_data = read_resource_file("reader/deprecated_pending_data.json");
    let mock_pending = server
        .mock("GET", "/feeder_gateway/get_state_update?blockNumber=pending&includeBlock=true")
        .with_status(200)
        .with_body(&raw_pending_data)
        .create_async()
        .await;
    let pending_data = apollo_starknet_client.pending_data().await;
    mock_pending.assert_async().await;
    let expected_pending_data: PendingData = serde_json::from_str(&raw_pending_data).unwrap();
    assert_eq!(pending_data.unwrap().unwrap(), expected_pending_data);

    // Accepted on L2.
    let raw_pending_data = read_resource_file("reader/accepted_on_l2_deprecated_data.json");
    let mock_accepted = server
        .mock("GET", "/feeder_gateway/get_state_update?blockNumber=pending&includeBlock=true")
        .with_status(200)
        .with_body(&raw_pending_data)
        .create_async()
        .await;
    let pending_data = apollo_starknet_client.pending_data().await;
    mock_accepted.assert_async().await;
    let expected_pending_data: PendingData = serde_json::from_str(&raw_pending_data).unwrap();
    assert_eq!(pending_data.unwrap().unwrap(), expected_pending_data);
}

#[tokio::test]
async fn get_block() {
    let mut server = mockito::Server::new_async().await;
    let apollo_starknet_client = apollo_starknet_client(&server);
    let json_filename = "reader/block_post_0_14_0.json";

    let mock_block = mock_successful_get_block_response(&mut server, json_filename, Some(20)).await;
    let block = apollo_starknet_client.block(BlockNumber(20)).await.unwrap().unwrap();
    mock_block.assert_async().await;

    let expected_block: Block = serde_json::from_str(&read_resource_file(json_filename)).unwrap();
    assert_eq!(block, expected_block);
}

// Requesting a block that does not exist, expecting a "Block Not Found" error.
#[tokio::test]
async fn get_block_not_found() {
    let mut server = mockito::Server::new_async().await;
    let apollo_starknet_client = apollo_starknet_client(&server);
    let mock_no_block = mock_error_get_block_response(
        &mut server,
        block_not_found_error(9999999999),
        Some(9999999999),
    )
    .await;
    let block = apollo_starknet_client.block(BlockNumber(9999999999)).await.unwrap();
    mock_no_block.assert_async().await;
    assert!(block.is_none());
}

#[tokio::test]
async fn get_block_aborted_returns_error() {
    let mut server = mockito::Server::new_async().await;
    let apollo_starknet_client = apollo_starknet_client(&server);
    let json_filename = "reader/block_post_0_14_0.json";
    let mut raw_block: Value = serde_json::from_str(&read_resource_file(json_filename)).unwrap();
    let raw_block_obj = raw_block.as_object_mut().expect("Block JSON should be an object.");
    raw_block_obj.insert("status".to_string(), Value::String("ABORTED".to_string()));
    raw_block_obj.insert("block_number".to_string(), Value::Number(20.into()));

    let mock_block = server
        .mock("GET", get_block_url(Some(20)).as_str())
        .with_status(200)
        .with_body(serde_json::to_string(&raw_block).unwrap())
        .create_async()
        .await;

    let err = apollo_starknet_client.block(BlockNumber(20)).await.unwrap_err();
    mock_block.assert_async().await;
    assert_matches!(err, ReaderClientError::AbortedBlock { block_number } if block_number == BlockNumber(20));
}

#[tokio::test]
async fn compiled_class_by_hash() {
    let mut server = mockito::Server::new_async().await;
    let apollo_starknet_client = apollo_starknet_client(&server);
    let raw_casm_contract_class = read_resource_file("reader/casm_contract_class.json");
    let mock_casm_contract_class = server
        .mock(
            "GET",
            &format!(
                "/feeder_gateway/get_compiled_class_by_class_hash?blockNumber=pending&\
                 {CLASS_HASH_QUERY}=0x7"
            )[..],
        )
        .with_status(200)
        .with_body(&raw_casm_contract_class)
        .create_async()
        .await;
    let casm_contract_class =
        apollo_starknet_client.compiled_class_by_hash(class_hash!("0x7")).await.unwrap().unwrap();
    mock_casm_contract_class.assert_async().await;
    let expected_casm_contract_class: CasmContractClass =
        serde_json::from_str(&raw_casm_contract_class).unwrap();
    assert_eq!(casm_contract_class, expected_casm_contract_class);

    let body = r#"{"code": "StarknetErrorCode.UNDECLARED_CLASS", "message": "Class with hash 0x7 is not declared."}"#;
    let mock_undeclared = server
        .mock(
            "GET",
            &format!(
                "/feeder_gateway/get_compiled_class_by_class_hash?blockNumber=pending&\
                 {CLASS_HASH_QUERY}=0x0"
            )[..],
        )
        .with_status(400)
        .with_body(body)
        .create_async()
        .await;
    let class = apollo_starknet_client.compiled_class_by_hash(class_hash!("0x0")).await.unwrap();
    mock_undeclared.assert_async().await;
    assert!(class.is_none());
}

#[tokio::test]
async fn is_alive() {
    let mut server = mockito::Server::new_async().await;
    let apollo_starknet_client = apollo_starknet_client(&server);
    let mock_is_alive = server
        .mock("GET", "/feeder_gateway/is_alive")
        .with_status(200)
        .with_body(FEEDER_GATEWAY_ALIVE_RESPONSE)
        .create_async()
        .await;
    let response = apollo_starknet_client.is_alive().await;
    mock_is_alive.assert_async().await;
    assert!(response);
}

// Empty storage diffs were filtered out in the past, but should not anymore (part of the inputs to
// the state diff commitment).
#[tokio::test]
async fn state_update_with_empty_storage_diff() {
    let mut server = mockito::Server::new_async().await;
    let apollo_starknet_client = apollo_starknet_client(&server);

    let mut state_update = StateUpdate::default();
    let empty_storage_diff = indexmap!(ContractAddress::default() => vec![]);
    state_update.state_diff.storage_diffs.clone_from(&empty_storage_diff);

    let mock = server
        .mock("GET", &format!("/feeder_gateway/get_state_update?{BLOCK_NUMBER_QUERY}=123456")[..])
        .with_status(200)
        .with_body(serde_json::to_string(&state_update).unwrap())
        .create_async()
        .await;
    let state_update =
        apollo_starknet_client.state_update(BlockNumber(123456)).await.unwrap().unwrap();
    mock.assert_async().await;
    assert_eq!(state_update.state_diff.storage_diffs, empty_storage_diff);
}

async fn test_unserializable<
    Output: Send + Debug,
    Fut: Future<Output = ReaderClientResult<Output>>,
    F: FnOnce(StarknetFeederGatewayClient) -> Fut,
>(
    server: &mut ServerGuard,
    url_suffix: &str,
    call_method: F,
) {
    let apollo_starknet_client = apollo_starknet_client(server);
    let body = "body";
    let mock = server.mock("GET", url_suffix).with_status(200).with_body(body).create_async().await;
    let error = call_method(apollo_starknet_client).await.unwrap_err();
    mock.assert_async().await;
    assert_matches!(error, ReaderClientError::SerdeError(_));
}

#[tokio::test]
async fn latest_block_unserializable() {
    let mut server = mockito::Server::new_async().await;
    test_unserializable(&mut server, &get_block_url(None), |apollo_starknet_client| async move {
        apollo_starknet_client.latest_block().await
    })
    .await
}

#[tokio::test]
async fn block_unserializable() {
    let mut server = mockito::Server::new_async().await;
    test_unserializable(
        &mut server,
        &get_block_url(Some(20)),
        |apollo_starknet_client| async move { apollo_starknet_client.block(BlockNumber(20)).await },
    )
    .await
}

#[tokio::test]
async fn class_by_hash_unserializable() {
    let mut server = mockito::Server::new_async().await;
    test_unserializable(
        &mut server,
        &format!("/feeder_gateway/get_class_by_hash?blockNumber=pending&{CLASS_HASH_QUERY}=0x1")[..],
        |apollo_starknet_client| async move {
            apollo_starknet_client.class_by_hash(class_hash!("0x1")).await
        },
    )
    .await
}

#[tokio::test]
async fn state_update_unserializable() {
    let mut server = mockito::Server::new_async().await;
    test_unserializable(
        &mut server,
        &format!("/feeder_gateway/get_state_update?{BLOCK_NUMBER_QUERY}=123456")[..],
        |apollo_starknet_client| async move {
            apollo_starknet_client.state_update(BlockNumber(123456)).await
        },
    )
    .await
}

#[tokio::test]
async fn compiled_class_by_hash_unserializable() {
    let mut server = mockito::Server::new_async().await;
    test_unserializable(
        &mut server,
        &format!(
            "/feeder_gateway/get_compiled_class_by_class_hash?blockNumber=pending&\
             {CLASS_HASH_QUERY}=0x7"
        )[..],
        |apollo_starknet_client| async move {
            apollo_starknet_client.compiled_class_by_hash(class_hash!("0x7")).await
        },
    )
    .await
}

#[tokio::test]
async fn get_block_signature() {
    let mut server = mockito::Server::new_async().await;
    let apollo_starknet_client = apollo_starknet_client(&server);

    let expected_block_signature = BlockSignatureData::Deprecated {
        block_number: BlockNumber(20),
        signature: [felt!("0x1"), felt!("0x2")],
        signature_input: BlockSignatureMessage {
            block_hash: BlockHash(felt!("0x20")),
            state_diff_commitment: GlobalRoot(felt!("0x1234")),
        },
    };

    let mock_block_signature = server
        .mock("GET", &format!("/feeder_gateway/get_signature?{BLOCK_NUMBER_QUERY}=20")[..])
        .with_status(200)
        .with_body(serde_json::to_string(&expected_block_signature).unwrap())
        .create_async()
        .await;

    let block_signature =
        apollo_starknet_client.block_signature(BlockNumber(20)).await.unwrap().unwrap();
    mock_block_signature.assert_async().await;
    assert_eq!(block_signature, expected_block_signature);
}

#[tokio::test]
async fn get_block_signature_unknown_block() {
    let mut server = mockito::Server::new_async().await;
    let apollo_starknet_client = apollo_starknet_client(&server);
    let mock_no_block = server
        .mock("GET", &format!("/feeder_gateway/get_signature?{BLOCK_NUMBER_QUERY}=999999")[..])
        .with_status(400)
        .with_body(block_not_found_error(999999))
        .create_async()
        .await;
    let block_signature =
        apollo_starknet_client.block_signature(BlockNumber(999999)).await.unwrap();
    mock_no_block.assert_async().await;
    assert!(block_signature.is_none());
}

#[tokio::test]
async fn get_sequencer_public_key() {
    let mut server = mockito::Server::new_async().await;
    let apollo_starknet_client = apollo_starknet_client(&server);

    let expected_sequencer_pub_key = SequencerPublicKey(PublicKey(felt!("0x1")));

    let mock_key = server
        .mock("GET", "/feeder_gateway/get_public_key")
        .with_status(200)
        .with_body(serde_json::to_string(&expected_sequencer_pub_key).unwrap())
        .create_async()
        .await;

    let pub_key = apollo_starknet_client.sequencer_pub_key().await.unwrap();
    mock_key.assert_async().await;
    assert_eq!(pub_key, expected_sequencer_pub_key);
}

#[tokio::test]
async fn get_block_compressed_and_uncompressed_are_equal() {
    use std::io::Write;

    use flate2::write::GzEncoder;
    use flate2::Compression;

    let raw_block = read_resource_file("reader/block_post_0_14_0.json");

    // Gzip-compress the response body.
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(raw_block.as_bytes()).unwrap();
    let compressed_body = encoder.finish().unwrap();

    // Fetch with compression enabled (gzip-encoded response).
    let mut compressed_server = mockito::Server::new_async().await;
    let compressed_client = apollo_starknet_client_with_compression(&compressed_server);
    let mock_compressed = compressed_server
        .mock("GET", get_block_url(Some(20)).as_str())
        .match_header("accept-encoding", mockito::Matcher::Regex("gzip".to_string()))
        .with_status(200)
        .with_header("content-encoding", "gzip")
        .with_body(compressed_body)
        .create_async()
        .await;
    let block_from_compressed = compressed_client.block(BlockNumber(20)).await.unwrap().unwrap();
    mock_compressed.assert_async().await;

    // Fetch without compression (plain JSON response).
    let mut plain_server = mockito::Server::new_async().await;
    let plain_client = apollo_starknet_client(&plain_server);
    let mock_plain = plain_server
        .mock("GET", get_block_url(Some(20)).as_str())
        .with_status(200)
        .with_body(&raw_block)
        .create_async()
        .await;
    let block_from_plain = plain_client.block(BlockNumber(20)).await.unwrap().unwrap();
    mock_plain.assert_async().await;

    // Both paths must produce the same result.
    assert_eq!(block_from_compressed, block_from_plain);
}
