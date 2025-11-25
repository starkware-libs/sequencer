use apollo_starknet_client::reader::{StarknetFeederGatewayClient, StarknetReader};
use apollo_starknet_client::retry::RetryConfig;
use serde::Serialize;
use starknet_api::block::BlockNumber;
use starknet_api::class_hash;
use tokio::join;

#[derive(Serialize)]
// Blocks with API changes to be tested with the get_block function.
struct BlocksForGetBlock {
    // First block, the original definitions.
    first_block: u32,
    // A block with declare transaction. (added in v0.9.0).
    declare_tx: u32,
    // A block with starknet version. (added in v0.9.1).
    starknet_version: u32,
    // A block with declare transaction version 1. (added in v0.10.0).
    // A block with nonce field in transaction. (added in v0.10.0).
    declare_version_1: u32,
    // A block with invoke_function transaction version 1 (added in v0.10.0).
    invoke_version_1: u32,
    // A block with deploy_account transaction. (added in v0.10.1).
    deploy_account: u32,
    // A block with declare transaction version 2. (added in v0.11.0).
    declare_version_2: u32,
}

#[derive(Serialize)]
// Blocks with API changes to be tested with the get_state_update function.
struct BlocksForGetStateUpdate {
    // First block, the original definitions.
    first_block: u32,
    // A state update with 'old_declared_contracts'. (added in v0.9.1).
    old_declared_contracts: u32,
    // A state update with 'nonces'. (added in v0.10.0).
    nonces: u32,
    // A state update with 'declared_classes'. (added in v0.11.0).
    declared_classes: u32,
    // A state update with 'replaced_classes'. (added in v0.11.0).
    replaced_classes: u32,
}

#[derive(Serialize)]
// Class hashes of different versions.
struct ClassHashes {
    // A class definition of Cairo 0 contract.
    cairo_0_class_hash: String,
    // A class definition of Cairo 1 contract. (added in v0.11.0).
    cairo_1_class_hash: String,
}

// Test data for a specific testnet.
struct TestEnvData {
    url: String,
    get_blocks: BlocksForGetBlock,
    get_state_updates: BlocksForGetStateUpdate,
    class_hashes: ClassHashes,
}

fn into_block_number_vec<T: Serialize>(obj: T) -> Vec<BlockNumber> {
    serde_json::to_value(obj)
        .unwrap()
        .as_object()
        .unwrap()
        .values()
        .map(|block_number_json_val| BlockNumber(block_number_json_val.as_u64().unwrap()))
        .collect()
}

#[tokio::test]
async fn test_integration_testnet() {
    let _ = simple_logger::init_with_env();
    let integration_testnet_data = TestEnvData {
        url: "https://feeder.integration-sepolia.starknet.io".to_owned(),
        get_blocks: BlocksForGetBlock {
            first_block: 0,
            declare_tx: 171486,
            starknet_version: 192397,
            declare_version_1: 228224,
            invoke_version_1: 228208,
            deploy_account: 238699,
            declare_version_2: 285182,
        },
        get_state_updates: BlocksForGetStateUpdate {
            first_block: 0,
            old_declared_contracts: 209679,
            nonces: 228155,
            declared_classes: 285182,
            replaced_classes: 0, // No block with this API change yet.
        },
        class_hashes: ClassHashes {
            cairo_0_class_hash: "0x5c478ee27f2112411f86f207605b2e2c58cdb647bac0df27f660ef2252359c6"
                .to_owned(),
            cairo_1_class_hash: "0x5d4f123c53c7cfe4db2725ff77f52b7cc0293115175c4a6ae26b931bb33c973"
                .to_owned(),
        },
    };
    run(integration_testnet_data).await;
}

#[tokio::test]
async fn test_alpha_testnet() {
    let _ = simple_logger::init_with_env();
    let alpha_testnet_data = TestEnvData {
        url: "https://feeder.alpha-sepolia.starknet.io".to_owned(),
        get_blocks: BlocksForGetBlock {
            first_block: 0,
            declare_tx: 248971,
            starknet_version: 280000,
            declare_version_1: 330039,
            invoke_version_1: 330291,
            deploy_account: 385429,
            declare_version_2: 789048,
        },
        get_state_updates: BlocksForGetStateUpdate {
            first_block: 0,
            old_declared_contracts: 248971,
            nonces: 330039,
            declared_classes: 789048,
            replaced_classes: 788504,
        },
        class_hashes: ClassHashes {
            cairo_0_class_hash: "0x5c478ee27f2112411f86f207605b2e2c58cdb647bac0df27f660ef2252359c6"
                .to_owned(),
            cairo_1_class_hash: "0x5d4f123c53c7cfe4db2725ff77f52b7cc0293115175c4a6ae26b931bb33c973"
                .to_owned(),
        },
    };
    run(alpha_testnet_data).await;
}

async fn run(test_env_data: TestEnvData) {
    let apollo_starknet_client = StarknetFeederGatewayClient::new(
        &test_env_data.url,
        None,
        "",
        RetryConfig { retry_base_millis: 30, retry_max_delay_millis: 30000, max_retries: 10 },
    )
    .expect("Create new client");

    join!(
        test_get_block(&apollo_starknet_client, test_env_data.get_blocks),
        test_get_state_update(&apollo_starknet_client, test_env_data.get_state_updates),
        test_class_hash(&apollo_starknet_client, test_env_data.class_hashes),
        async { apollo_starknet_client.pending_data().await.unwrap().unwrap() },
    );
}

// Call get_block on the given list of block_numbers.
async fn test_get_block(
    apollo_starknet_client: &StarknetFeederGatewayClient,
    block_numbers: BlocksForGetBlock,
) {
    for block_number in into_block_number_vec(block_numbers) {
        apollo_starknet_client.block(block_number).await.unwrap().unwrap();
    }

    // Get the last block.
    apollo_starknet_client.latest_block().await.unwrap().unwrap();
    // Not existing block.
    assert!(apollo_starknet_client.block(BlockNumber(u64::MAX)).await.unwrap().is_none());
}

// Call get_state_update on the given list of block_numbers.
async fn test_get_state_update(
    apollo_starknet_client: &StarknetFeederGatewayClient,
    block_numbers: BlocksForGetStateUpdate,
) {
    for block_number in into_block_number_vec(block_numbers) {
        apollo_starknet_client.state_update(block_number).await.unwrap().unwrap();
    }
}

// Call class_by_hash for the given list of class_hashes.
async fn test_class_hash(
    apollo_starknet_client: &StarknetFeederGatewayClient,
    class_hashes: ClassHashes,
) {
    let data = serde_json::to_value(class_hashes).unwrap();

    for class_hash_json_val in data.as_object().unwrap().values() {
        let class_hash_val = class_hash_json_val.as_str().unwrap();
        let class_hash = class_hash!(class_hash_val);
        apollo_starknet_client.class_by_hash(class_hash).await.unwrap().unwrap();
    }
}
