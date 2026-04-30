use std::path::PathBuf;
use std::sync::{Arc, LazyLock};
use std::{env, fs};

use apollo_batcher::cende_client_types::{CendeBlockMetadata, CendePreconfirmedBlock};
use apollo_batcher::pre_confirmed_cende_client::CendeWritePreconfirmedBlock;
use apollo_batcher_types::batcher_types::Round;
use apollo_class_manager_types::{MockClassManagerClient, SharedClassManagerClient};
use apollo_consensus::types::ProposalCommitment;
use apollo_consensus_orchestrator::cende::{
    AerospikeBlob,
    BlobParameters,
    InternalTransactionWithReceipt,
};
use apollo_infra_utils::compile_time_cargo_manifest_dir;
use blockifier::blockifier_versioned_constants::VersionedConstants;
use blockifier::bouncer::BouncerConfig;
use blockifier::context::{BlockContext, ChainInfo};
use blockifier::state::cached_state::StateMaps;
use blockifier::test_utils::dict_state_reader::DictStateReader;
use blockifier_test_utils::contracts::FeatureContract;
use expect_test::expect_file;
use google_cloud_storage::client::{Client, ClientConfig};
use google_cloud_storage::http::objects::download::Range;
use google_cloud_storage::http::objects::get::GetObjectRequest;
use google_cloud_storage::http::objects::upload::{Media, UploadObjectRequest, UploadType};
use starknet_api::block::{BlockHash, BlockHashAndNumber, BlockInfo, BlockNumber, BlockTimestamp};
use starknet_api::block_hash::block_hash_calculator::PartialBlockHashComponents;
use starknet_api::consensus_transaction::InternalConsensusTransaction;
use starknet_api::contract_address;
use starknet_api::core::{ChainId, OsChainInfo};
use starknet_api::executable_transaction::AccountTransaction as ExecutableAccountTx;
use starknet_api::hash::StateRoots;
use starknet_api::state::ThinStateDiff;
use starknet_api::test_utils::TEST_SEQUENCER_ADDRESS;
use starknet_patricia_storage::map_storage::MapStorage;

const BLOBS_BUCKET_NAME: &str = "apollo-central-systest-blobs";
const BLOBS_FILE_NAME: &str = "blobs.json";
const BLOBS_GENERATION_FILE: LazyLock<PathBuf> = LazyLock::new(|| {
    PathBuf::from(compile_time_cargo_manifest_dir!()).join("resources/blob_file_generation")
});

const N_TXS_PER_BLOCK: usize = 1;
static CHAIN_ID: LazyLock<ChainId> =
    LazyLock::new(|| ChainId::Other("SN_PREINTEGRATION_SEPOLIA".to_string()));
static CHAIN_INFO: LazyLock<ChainInfo> =
    LazyLock::new(|| ChainInfo { chain_id: CHAIN_ID.clone(), ..ChainInfo::create_for_testing() });

const CHAIN_INFO_PATH: &str = "../resources/chain_info.json";
const PRECONFIRMED_BLOCK_PATH: &str = "../resources/preconfirmed_block.json";

type TxPair = (ExecutableAccountTx, InternalConsensusTransaction);

/// ID of the current blobs file.
fn current_generation() -> usize {
    fs::read_to_string(&*BLOBS_GENERATION_FILE)
        .unwrap_or_else(|error| panic!("Failed to read file {BLOBS_GENERATION_FILE:?}: {error}"))
        .trim()
        .parse()
        .unwrap()
}

fn blobs_object_path(generation: usize) -> String {
    format!("{generation}/{BLOBS_FILE_NAME}")
}

// =====================
// Tx generation
// =====================

#[expect(dead_code)]
fn boostrap_declare_tx(
    _class_manager: &mut MockClassManagerClient,
    _contract: FeatureContract,
) -> TxPair {
    unimplemented!()
}

fn make_txs() -> (MockClassManagerClient, Vec<TxPair>) {
    // TODO(Dori): implement.
    (MockClassManagerClient::default(), vec![])
}

// =====================
// Data generation
// =====================

#[expect(dead_code)]
fn make_block_context(block_number: usize) -> BlockContext {
    BlockContext::new(
        BlockInfo {
            block_number: BlockNumber(u64::try_from(block_number).unwrap()),
            block_timestamp: BlockTimestamp(1000 + u64::try_from(block_number).unwrap()),
            sequencer_address: contract_address!(TEST_SEQUENCER_ADDRESS),
            ..Default::default()
        },
        CHAIN_INFO.clone(),
        VersionedConstants::create_for_testing(),
        BouncerConfig::max(),
    )
}

/// Executes the transactions and applies the changes to the state.
#[expect(dead_code)]
fn execute_block(
    _state: &mut DictStateReader,
    _block_context: &BlockContext,
    _old_block_number_and_hash: Option<BlockHashAndNumber>,
    _txs: &[TxPair],
) -> (Vec<InternalTransactionWithReceipt>, StateMaps) {
    unimplemented!()
}

#[expect(dead_code)]
async fn compute_block_hash_components(
    _block_info: &BlockInfo,
    _state_diff: &ThinStateDiff,
    _txs: &[InternalTransactionWithReceipt],
) -> PartialBlockHashComponents {
    unimplemented!()
}

/// Given previous state and partial components, commits the changes and finalizes the block hash.
/// Returns the block hash, the new state roots and the updated committer storage.
#[expect(dead_code)]
async fn compute_block_commitments(
    _committer_storage: MapStorage,
    _prev_state_roots: &StateRoots,
    _state_maps: &StateMaps,
    _block_hash_components: PartialBlockHashComponents,
    _prev_block_hash: BlockHash,
) -> (BlockHash, StateRoots, MapStorage) {
    unimplemented!()
}

/// Creates a blob for the given block.
/// If this is not the first block, also sets the parent proposal commitment and populates the
/// recent block hashes with the last block hash (of the previous block).
/// Returns the current proposal commitment and the block hash components (for use in block hash
/// computation of the current block).
#[expect(dead_code)]
async fn make_blob_parameters(
    _block_context: &BlockContext,
    _txs_with_exec: Vec<InternalTransactionWithReceipt>,
    _state_maps: &StateMaps,
    _parent_data: (BlockHash, ProposalCommitment),
) -> (BlobParameters, PartialBlockHashComponents, ProposalCommitment) {
    unimplemented!()
}

/// Creates a preconfirmed block for the given block. Should be called for the last block only - no
/// commitment is computed.
fn make_preconfirmed_block(
    block_number: usize,
    _state: &mut DictStateReader,
    _txs: &[TxPair],
) -> CendeWritePreconfirmedBlock {
    // TODO(Dori): implement.
    CendeWritePreconfirmedBlock {
        block_number: BlockNumber(u64::try_from(block_number).unwrap()),
        round: Round::default(),
        write_iteration: 0,
        pre_confirmed_block: CendePreconfirmedBlock {
            metadata: CendeBlockMetadata::new(BlockInfo::default()),
            transactions: vec![],
            transaction_receipts: vec![],
            transaction_state_diffs: vec![],
        },
    }
}

/// Given a list of blocks (block number and contents), executes the transactions and creates the
/// blobs.
async fn make_blobs(
    _blocks_to_commit: &[(usize, &[TxPair])],
    _state: &mut DictStateReader,
    _shared_class_manager: SharedClassManagerClient,
) -> Vec<AerospikeBlob> {
    // TODO(Dori): implement.
    vec![]
}

/// Generates a fixed set of blob data, and one preconfirmed block, with a deterministic list of
/// transactions.
async fn make_data() -> (Vec<AerospikeBlob>, CendeWritePreconfirmedBlock) {
    let (class_manager, transactions) = make_txs();
    let shared_class_manager = Arc::new(class_manager);
    let mut state = DictStateReader::default();

    // TODO(Dori): remove this case, it should never happen when the test is done.
    if transactions.is_empty() {
        (vec![], make_preconfirmed_block(0, &mut state, &[]))
    } else {
        let block_iterator = transactions.chunks(N_TXS_PER_BLOCK).enumerate().collect::<Vec<_>>();
        // Split the block iterator into two iterators: one for the blocks to be committed, and one
        // for the last block.
        let (blocks_to_commit, last_block) = block_iterator.split_at(block_iterator.len() - 1);
        let (last_block_number, last_block_txs) = last_block.last().unwrap();

        let blobs = make_blobs(blocks_to_commit, &mut state, shared_class_manager.clone()).await;
        // For the last block, create a preconfirmed block.
        let preconfirmed_block =
            make_preconfirmed_block(*last_block_number, &mut state, last_block_txs);

        (blobs, preconfirmed_block)
    }
}

// =====================
// Blob file storage
// =====================

async fn gcs_client() -> Client {
    Client::new(ClientConfig::default().with_auth().await.expect(
        "Failed to create GCS client config. Did you run `gcloud auth application-default login`?",
    ))
}

/// Pushes the blobs to GCS.
async fn bump_generation_and_store_blob_file(blobs: &[AerospikeBlob], client: &Client) {
    let blobs_json = serde_json::to_string_pretty(blobs).unwrap();
    let next_generation = current_generation() + 1;
    client
        .upload_object(
            &UploadObjectRequest {
                bucket: BLOBS_BUCKET_NAME.to_string(),
                // Don't overwrite any existing file.
                if_generation_match: Some(0),
                ..Default::default()
            },
            blobs_json.into_bytes(),
            &UploadType::Simple(Media::new(blobs_object_path(next_generation))),
        )
        .await
        .unwrap();
    fs::write(&*BLOBS_GENERATION_FILE, next_generation.to_string()).unwrap();
}

/// Fetches the blobs from GCS.
async fn fetch_blob_file(client: &Client) -> Vec<AerospikeBlob> {
    let blobs_json = client
        .download_object(
            &GetObjectRequest {
                bucket: BLOBS_BUCKET_NAME.to_string(),
                object: blobs_object_path(current_generation()),
                ..Default::default()
            },
            &Range::default(),
        )
        .await
        .unwrap();
    serde_json::from_slice(&blobs_json).unwrap()
}

// =====================
// Test
// =====================

#[tokio::test]
async fn test_make_data() {
    let (blobs, preconfirmed_block) = make_data().await;
    let chain_info = OsChainInfo::from(&*CHAIN_INFO).to_hex_map();
    expect_file![CHAIN_INFO_PATH].assert_eq(&serde_json::to_string_pretty(&chain_info).unwrap());
    expect_file![PRECONFIRMED_BLOCK_PATH]
        .assert_eq(&serde_json::to_string_pretty(&preconfirmed_block).unwrap());

    // Upload or download blobs depending on the fix mode.
    let client = gcs_client().await;
    if env::var("UPDATE_EXPECT").is_ok() {
        bump_generation_and_store_blob_file(&blobs, &client).await;
    } else {
        let fetched_blobs = fetch_blob_file(&client).await;
        assert_eq!(
            blobs, fetched_blobs,
            "Blobs mismatch. To fix, run the test with UPDATE_EXPECT=1."
        );
    }
}
