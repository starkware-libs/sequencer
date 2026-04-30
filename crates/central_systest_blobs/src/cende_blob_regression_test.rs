use std::path::PathBuf;
use std::sync::LazyLock;
use std::{env, fs};

use apollo_batcher::cende_client_types::{
    CendeBlockMetadata,
    CendePreconfirmedBlock,
    CendePreconfirmedTransaction,
    StarknetClientStateDiff,
    StarknetClientTransactionReceipt,
};
use apollo_batcher::pre_confirmed_cende_client::CendeWritePreconfirmedBlock;
use apollo_batcher_types::batcher_types::Round;
use apollo_class_manager_types::MockClassManagerClient;
use apollo_consensus_orchestrator::cende::{
    AerospikeBlob,
    BlobParameters,
    InternalTransactionWithReceipt,
};
use apollo_infra_utils::compile_time_cargo_manifest_dir;
use blockifier::blockifier_versioned_constants::VersionedConstants;
use blockifier::bouncer::BouncerConfig;
use blockifier::context::{BlockContext, ChainInfo};
use blockifier::state::cached_state::{CachedState, StateMaps};
use blockifier::test_utils::contracts::FeatureContractTrait;
use blockifier::test_utils::dict_state_reader::DictStateReader;
use blockifier::transaction::account_transaction::AccountTransaction as BlockifierAccountTx;
use blockifier::transaction::transactions::ExecutableTransaction;
use blockifier_test_utils::contracts::FeatureContract;
use expect_test::expect_file;
use google_cloud_storage::client::{Client, ClientConfig};
use google_cloud_storage::http::error::ErrorResponse;
use google_cloud_storage::http::objects::download::Range;
use google_cloud_storage::http::objects::get::GetObjectRequest;
use google_cloud_storage::http::objects::upload::{Media, UploadObjectRequest, UploadType};
use google_cloud_storage::http::Error as GcsError;
use mockall::predicate::eq;
use starknet_api::block::{BlockHash, BlockInfo, BlockNumber, BlockTimestamp};
use starknet_api::block_hash::block_hash_calculator::PartialBlockHashComponents;
use starknet_api::consensus_transaction::InternalConsensusTransaction;
use starknet_api::contract_address;
use starknet_api::contract_class::compiled_class_hash::HashVersion;
use starknet_api::core::{ChainId, Nonce, OsChainInfo};
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::executable_transaction::{
    AccountTransaction as ExecutableAccountTx,
    DeclareTransaction as ExecutableDeclareTransaction,
};
use starknet_api::hash::StateRoots;
use starknet_api::rpc_transaction::{
    InternalRpcDeclareTransactionV3,
    InternalRpcTransaction,
    InternalRpcTransactionWithoutTxHash,
};
use starknet_api::test_utils::TEST_SEQUENCER_ADDRESS;
use starknet_api::transaction::fields::{
    AccountDeploymentData,
    AllResourceBounds,
    PaymasterData,
    Tip,
    TransactionSignature,
};
use starknet_api::transaction::{
    DeclareTransaction,
    TransactionHasher,
    TransactionOffsetInBlock,
    TransactionVersion,
};
use starknet_committer::db::facts_db::FactsDb;
use starknet_committer::db::forest_trait::StorageInitializer;
use starknet_patricia_storage::map_storage::MapStorage;

const GCS_ERROR_CODE_NOT_FOUND: u16 = 404;

const BLOBS_BUCKET_NAME: &str = "apollo-central-systest-blobs";
const BLOBS_FILE_NAME: &str = "blobs.json";
static BLOBS_GENERATION_FILE: LazyLock<PathBuf> = LazyLock::new(|| {
    PathBuf::from(compile_time_cargo_manifest_dir!()).join("resources/blob_file_generation")
});

static CHAIN_ID: LazyLock<ChainId> =
    LazyLock::new(|| ChainId::Other("SN_PREINTEGRATION_SEPOLIA".to_string()));

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

#[expect(dead_code)]
struct BlockData {
    block_context: BlockContext,
    transactions_with_receipts: Vec<InternalTransactionWithReceipt>,
    partial_block_hash_components: PartialBlockHashComponents,
    block_hash: BlockHash,
    state_maps: StateMaps,
    state_roots: StateRoots,
}

struct BlobFactory {
    chain_info: ChainInfo,
    class_manager: MockClassManagerClient,

    // Finalized blocks.
    blocks: Vec<BlockData>,

    // Transactions for the next block.
    next_txs: Vec<TxPair>,

    // Context.
    state: DictStateReader,
    #[expect(dead_code)]
    committer_storage: FactsDb<MapStorage>,
}

impl BlobFactory {
    pub fn new() -> Self {
        let chain_info =
            ChainInfo { chain_id: CHAIN_ID.clone(), ..ChainInfo::create_for_testing() };
        Self {
            chain_info,
            class_manager: MockClassManagerClient::default(),
            blocks: vec![],
            next_txs: vec![],
            state: DictStateReader::default(),
            committer_storage: FactsDb::new(MapStorage::default()),
        }
    }

    /// Executes the unblocked transactions and applies the changes to the state.
    #[expect(dead_code)]
    fn close_block(&mut self) {
        unimplemented!()
    }

    /// Creates blobs for all finalized blocks, and a preconfirmed block with the remaining txs that
    /// were not included in a block.
    async fn finalize(self) -> (Vec<AerospikeBlob>, CendeWritePreconfirmedBlock) {
        // TODO(Dori): Create the blob vector.
        let blobs = vec![];

        // For the last block, create a preconfirmed block.
        let preconfirmed_block = self.make_preconfirmed_block_from_remaining_txs();

        (blobs, preconfirmed_block)
    }

    #[expect(dead_code)]
    fn parent_block_hash(&self) -> BlockHash {
        self.blocks.last().map(|block| block.block_hash).unwrap_or(BlockHash::GENESIS_PARENT_HASH)
    }

    #[expect(dead_code)]
    fn current_state_roots(&self) -> StateRoots {
        self.blocks.last().map(|block| block.state_roots).unwrap_or(StateRoots::default())
    }

    // =====================
    // Tx generation
    // =====================

    #[expect(dead_code)]
    fn boostrap_declare_tx(&mut self, contract: FeatureContract) {
        let sender_address = ExecutableDeclareTransaction::bootstrap_address();
        let sierra = contract.get_sierra();
        let class_hash = sierra.calculate_class_hash();
        let compiled_class_hash = contract.get_compiled_class_hash(&HashVersion::V2);
        let resource_bounds = AllResourceBounds::new_unlimited_gas_no_fee_enforcement();
        let nonce = Nonce::default();

        // Create internal tx.
        let internal_declare_without_hash = InternalRpcDeclareTransactionV3 {
            sender_address,
            nonce,
            class_hash,
            compiled_class_hash,
            resource_bounds,
            signature: TransactionSignature::default(),
            tip: Tip::default(),
            paymaster_data: PaymasterData::default(),
            account_deployment_data: AccountDeploymentData::default(),
            nonce_data_availability_mode: DataAvailabilityMode::L1,
            fee_data_availability_mode: DataAvailabilityMode::L1,
        };
        let tx_hash = internal_declare_without_hash
            .calculate_transaction_hash(&CHAIN_ID, &TransactionVersion::THREE)
            .unwrap();
        let internal_tx = InternalConsensusTransaction::RpcTransaction(InternalRpcTransaction {
            tx: InternalRpcTransactionWithoutTxHash::Declare(internal_declare_without_hash.clone()),
            tx_hash,
        });

        // Create executable tx.
        let executable = ExecutableDeclareTransaction::create(
            DeclareTransaction::V3(internal_declare_without_hash.into()),
            contract.get_class_info(),
            &CHAIN_ID,
        )
        .unwrap();

        // Mock the class manager.
        // The class manager methods may not be called if a blob is not created with this declare.
        self.class_manager
            .expect_get_sierra()
            .with(eq(class_hash))
            .times(..=1)
            .returning(move |_| Ok(Some(sierra.clone())));
        self.class_manager
            .expect_get_executable()
            .with(eq(class_hash))
            .times(..=1)
            .returning(move |_| Ok(Some(contract.get_class())));

        // Return the transactions.
        self.next_txs.push((executable.into(), internal_tx));
    }

    // =====================
    // Data generation
    // =====================

    fn next_block_context(&self) -> BlockContext {
        let block_number = BlockNumber(u64::try_from(self.blocks.len()).unwrap());
        BlockContext::new(
            BlockInfo {
                block_number,
                block_timestamp: BlockTimestamp(1000 + block_number.0),
                sequencer_address: contract_address!(TEST_SEQUENCER_ADDRESS),
                ..Default::default()
            },
            self.chain_info.clone(),
            VersionedConstants::create_for_testing(),
            BouncerConfig::max(),
        )
    }

    /// Creates a blob for the given block.
    /// If this is not the first block, also sets the parent proposal commitment and populates the
    /// recent block hashes with the last block hash (of the previous block).
    /// Returns the current proposal commitment and the block hash components (for use in block hash
    /// computation of the current block).
    #[expect(dead_code)]
    async fn make_blob_parameters(&self, _block: &BlockData) -> BlobParameters {
        unimplemented!()
    }

    /// Creates a preconfirmed block for the given block. Should be called for the last block only -
    /// no commitment is computed.
    fn make_preconfirmed_block_from_remaining_txs(&self) -> CendeWritePreconfirmedBlock {
        let block_context = self.next_block_context();
        let block_info = block_context.block_info().clone();
        let mut transactions = vec![];
        let mut transaction_receipts = vec![];
        let mut transaction_state_diffs = vec![];

        for (tx_index, (executable, internal)) in self.next_txs.iter().enumerate() {
            let tx_hash = match &internal {
                InternalConsensusTransaction::RpcTransaction(tx) => tx.tx_hash,
                InternalConsensusTransaction::L1Handler(_) => {
                    panic!("unexpected L1Handler in test")
                }
            };

            let mut tx_state = CachedState::new(self.state.clone());
            let execution_info = BlockifierAccountTx::new_for_sequencing(executable.clone())
                .execute(&mut tx_state, &block_context)
                .unwrap();

            let state_changes = tx_state.to_state_diff().unwrap();

            let receipt = StarknetClientTransactionReceipt::from((
                tx_hash,
                TransactionOffsetInBlock(tx_index),
                &execution_info,
                None,
            ));
            let tx_state_diff = StarknetClientStateDiff::from(state_changes.state_maps).0;

            transactions.push(CendePreconfirmedTransaction::from(internal.clone()));
            transaction_receipts.push(Some(receipt));
            transaction_state_diffs.push(Some(tx_state_diff));
        }

        CendeWritePreconfirmedBlock {
            block_number: block_info.block_number,
            round: Round::default(),
            write_iteration: 0,
            pre_confirmed_block: CendePreconfirmedBlock {
                metadata: CendeBlockMetadata::new(block_info),
                transactions,
                transaction_receipts,
                transaction_state_diffs,
            },
        }
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

async fn find_next_available_blobs_generation(client: &Client) -> usize {
    let mut next_generation = current_generation() + 1;
    loop {
        match fetch_raw_blobs_at_generation(client, next_generation).await {
            Ok(_) => next_generation += 1,
            Err(GcsError::Response(ErrorResponse { code: GCS_ERROR_CODE_NOT_FOUND, .. })) => break,
            Err(e) => panic!("Failed to fetch blobs at generation {next_generation}: {e}"),
        }
    }
    next_generation
}

async fn fetch_raw_blobs_at_generation(
    client: &Client,
    generation: usize,
) -> Result<Vec<u8>, GcsError> {
    client
        .download_object(
            &GetObjectRequest {
                bucket: BLOBS_BUCKET_NAME.to_string(),
                object: blobs_object_path(generation),
                ..Default::default()
            },
            &Range::default(),
        )
        .await
}

/// Pushes the blobs to GCS.
async fn bump_generation_and_store_blob_file(blobs: &[AerospikeBlob], client: &Client) {
    let blobs_json = serde_json::to_string_pretty(blobs).unwrap();
    let next_generation = find_next_available_blobs_generation(client).await;
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
    let blobs_json = fetch_raw_blobs_at_generation(client, current_generation()).await.unwrap();
    serde_json::from_slice(&blobs_json).unwrap()
}

// =====================
// Test
// =====================

#[tokio::test]
async fn test_make_data() {
    let blob_factory = BlobFactory::new();
    let chain_info = OsChainInfo::from(&blob_factory.chain_info).to_hex_map();
    // TODO(Dori): create txs.
    let (blobs, preconfirmed_block) = blob_factory.finalize().await;
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
