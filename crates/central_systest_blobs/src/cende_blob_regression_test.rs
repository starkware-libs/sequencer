use std::sync::{Arc, LazyLock};

use apollo_batcher::cende_client_types::{
    CendeBlockMetadata,
    CendePreconfirmedBlock,
    CendePreconfirmedTransaction,
    StarknetClientStateDiff,
    StarknetClientTransactionReceipt,
};
use apollo_batcher::pre_confirmed_cende_client::CendeWritePreconfirmedBlock;
use apollo_batcher_types::batcher_types::Round;
use apollo_class_manager_types::{MockClassManagerClient, SharedClassManagerClient};
use apollo_consensus::types::ProposalCommitment;
use apollo_consensus_orchestrator::cende::{
    AerospikeBlob,
    BlobParameters,
    InternalTransactionWithReceipt,
};
use apollo_consensus_orchestrator::fee_market::FeeMarketInfo;
use blockifier::abi::constants::STORED_BLOCK_HASH_BUFFER;
use blockifier::blockifier_versioned_constants::VersionedConstants;
use blockifier::bouncer::{BouncerConfig, BouncerWeights, CasmHashComputationData};
use blockifier::context::{BlockContext, ChainInfo};
use blockifier::state::cached_state::{CachedState, CommitmentStateDiff, StateMaps};
use blockifier::state::state_api::UpdatableState;
use blockifier::test_utils::contracts::FeatureContractTrait;
use blockifier::test_utils::dict_state_reader::DictStateReader;
use blockifier::transaction::account_transaction::AccountTransaction as BlockifierAccountTx;
use blockifier::transaction::transactions::ExecutableTransaction;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::contracts::FeatureContract;
use expect_test::expect_file;
use mockall::predicate::eq;
use starknet_api::block::{BlockHash, BlockHashAndNumber, BlockInfo, BlockNumber, BlockTimestamp};
use starknet_api::block_hash::block_hash_calculator::{
    PartialBlockHash,
    PartialBlockHashComponents,
};
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
use starknet_api::state::ThinStateDiff;
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
use starknet_patricia_storage::map_storage::MapStorage;

const N_TXS_PER_BLOCK: usize = 1;
static CHAIN_ID: LazyLock<ChainId> =
    LazyLock::new(|| ChainId::Other("SN_PREINTEGRATION_SEPOLIA".to_string()));
static CHAIN_INFO: LazyLock<ChainInfo> =
    LazyLock::new(|| ChainInfo { chain_id: CHAIN_ID.clone(), ..ChainInfo::create_for_testing() });

const CHAIN_INFO_PATH: &str = "../resources/chain_info.json";
const BLOB_LIST_PATH: &str = "../resources/blobs.json";
const PRECONFIRMED_BLOCK_PATH: &str = "../resources/preconfirmed_block.json";

type TxPair = (ExecutableAccountTx, InternalConsensusTransaction);

// =====================
// Tx generation
// =====================

fn boostrap_declare_tx(
    class_manager: &mut MockClassManagerClient,
    contract: FeatureContract,
) -> TxPair {
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
    let internal = InternalConsensusTransaction::RpcTransaction(InternalRpcTransaction {
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
    class_manager
        .expect_get_sierra()
        .with(eq(class_hash))
        .times(..=1)
        .returning(move |_| Ok(Some(sierra.clone())));
    class_manager
        .expect_get_executable()
        .with(eq(class_hash))
        .times(..=1)
        .returning(move |_| Ok(Some(contract.get_class())));

    // Return the transactions.
    (executable.into(), internal)
}

fn make_txs() -> (MockClassManagerClient, Vec<TxPair>) {
    // Create the list of transactions to be included in the blobs:
    // 1. bootstrap declare of an ERC20 contract.
    // 2. bootstrap declare of an account with real validate.
    // TODO(Dori): the rest of the txs.
    // 3. deploy account (with zero fees).
    // 4. deploy ERC20 contract from the account (with zero fees), while minting some tokens to the
    //    sender account.
    // (from this point - all txs include non-zero fees, and no more bootstrap declares)
    // 5. declare the test contract.
    // 6. deploy the test contract.
    // 7. deploy another instance of the test contract.
    // 8. invoke the test contract: something with a state change.
    // 9. invoke the test contract: test syscalls.

    let mut class_manager = MockClassManagerClient::new();
    let erc20_contract = FeatureContract::ERC20(CairoVersion::Cairo1(RunnableCairo1::Casm));
    let account_with_real_validate = FeatureContract::AccountWithRealValidate(RunnableCairo1::Casm);

    let erc20_declare_tx = boostrap_declare_tx(&mut class_manager, erc20_contract);
    let account_with_real_validate_declare_tx =
        boostrap_declare_tx(&mut class_manager, account_with_real_validate);
    (class_manager, vec![erc20_declare_tx, account_with_real_validate_declare_tx])
}

// =====================
// Data generation
// =====================

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
fn execute_block(
    _state: &mut DictStateReader,
    _block_context: &BlockContext,
    _old_block_number_and_hash: Option<BlockHashAndNumber>,
    _txs: &[TxPair],
) -> (Vec<InternalTransactionWithReceipt>, StateMaps) {
    // TODO(Dori): implement.
    (vec![], StateMaps::default())
}

async fn compute_block_hash_components(
    _block_info: &BlockInfo,
    _state_diff: &ThinStateDiff,
    _txs: &[InternalTransactionWithReceipt],
) -> PartialBlockHashComponents {
    // TODO(Dori): implement.
    PartialBlockHashComponents::default()
}

/// Given previous state and partial components, commits the changes and finalizes the block hash.
/// Returns the block hash, the new state roots and the updated committer storage.
async fn compute_block_commitments(
    _committer_storage: MapStorage,
    _prev_state_roots: &StateRoots,
    _state_maps: &StateMaps,
    _block_hash_components: PartialBlockHashComponents,
    _prev_block_hash: BlockHash,
) -> (BlockHash, StateRoots, MapStorage) {
    // TODO(Dori): implement.
    (BlockHash::default(), StateRoots::default(), MapStorage::default())
}

/// Creates a blob for the given block.
/// If this is not the first block, also sets the parent proposal commitment and populates the
/// recent block hashes with the last block hash (of the previous block).
/// Returns the current proposal commitment and the block hash components (for use in block hash
/// computation of the current block).
async fn make_blob_parameters(
    block_context: &BlockContext,
    txs_with_exec: Vec<InternalTransactionWithReceipt>,
    state_maps: &StateMaps,
    parent_data: (BlockHash, ProposalCommitment),
) -> (BlobParameters, PartialBlockHashComponents, ProposalCommitment) {
    let commitment_state_diff = CommitmentStateDiff::from(state_maps.clone());
    let state_diff = ThinStateDiff::from(commitment_state_diff.clone());
    let block_info = block_context.block_info().clone();
    let block_hash_components =
        compute_block_hash_components(&block_info, &state_diff, &txs_with_exec).await;
    let proposal_commitment = ProposalCommitment(
        PartialBlockHash::from_partial_block_hash_components(&block_hash_components).unwrap().0,
    );

    let (recent_block_hashes, parent_proposal_commitment) = if block_info.block_number.0 > 0 {
        let (parent_block_hash, parent_proposal_commitment) = parent_data;
        (
            vec![BlockHashAndNumber {
                number: BlockNumber(block_info.block_number.0 - 1),
                hash: parent_block_hash,
            }],
            Some(parent_proposal_commitment),
        )
    } else {
        (vec![], None)
    };

    (
        BlobParameters {
            block_info,
            state_diff,
            compressed_state_diff: Some(commitment_state_diff),
            transactions_with_execution_infos: txs_with_exec,
            bouncer_weights: BouncerWeights::default(),
            fee_market_info: FeeMarketInfo::default(),
            casm_hash_computation_data_sierra_gas: CasmHashComputationData::default(),
            casm_hash_computation_data_proving_gas: CasmHashComputationData::default(),
            compiled_class_hashes_for_migration: vec![],
            proposal_commitment,
            parent_proposal_commitment,
            recent_block_hashes,
        },
        block_hash_components,
        proposal_commitment,
    )
}

/// Creates a preconfirmed block for the given block. Should be called for the last block only - no
/// commitment is computed.
fn make_preconfirmed_block(
    block_number: usize,
    state: &mut DictStateReader,
    txs: &[TxPair],
) -> CendeWritePreconfirmedBlock {
    let block_context = make_block_context(block_number);

    let mut transactions = vec![];
    let mut transaction_receipts = vec![];
    let mut transaction_state_diffs = vec![];

    for (tx_index, (executable, internal)) in txs.into_iter().enumerate() {
        let tx_hash = match &internal {
            InternalConsensusTransaction::RpcTransaction(tx) => tx.tx_hash,
            InternalConsensusTransaction::L1Handler(_) => panic!("unexpected L1Handler in test"),
        };

        let mut tx_state = CachedState::new(state.clone());
        let execution_info = BlockifierAccountTx::new_for_sequencing(executable.clone())
            .execute(&mut tx_state, &block_context)
            .unwrap();

        let state_changes = tx_state.to_state_diff().unwrap();
        let class_mapping = tx_state.class_hash_to_class.borrow().clone();
        state.apply_writes(&state_changes.state_maps, &class_mapping);

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
        block_number: BlockNumber(u64::try_from(block_number).unwrap()),
        round: Round::default(),
        write_iteration: 0,
        pre_confirmed_block: CendePreconfirmedBlock {
            metadata: CendeBlockMetadata::new(block_context.block_info().clone()),
            transactions,
            transaction_receipts,
            transaction_state_diffs,
        },
    }
}

/// Given a list of blocks (block number and contents), executes the transactions and creates the
/// blobs.
async fn make_blobs(
    blocks_to_commit: &[(usize, &[TxPair])],
    state: &mut DictStateReader,
    shared_class_manager: SharedClassManagerClient,
) -> Vec<AerospikeBlob> {
    let mut prev_block_hash = BlockHash::GENESIS_PARENT_HASH;
    let mut prev_state_roots = StateRoots::default();
    let mut prev_proposal_commitment = ProposalCommitment::default();
    let mut committer_storage = MapStorage::default();

    // "Mapping" from block number to block hash.
    let mut block_hashes = vec![];

    // Iterate over all except the last block.
    let mut blobs = vec![];
    for (block_number, txs_for_block) in blocks_to_commit {
        let block_context = make_block_context(*block_number);
        let u64_block_number = u64::try_from(*block_number).unwrap();

        // If the block number is after the block hash buffer, set the previous block hash and
        // number, so they appear in the state diff.
        let prev_block_hash_and_number = if u64_block_number < STORED_BLOCK_HASH_BUFFER {
            None
        } else {
            let old_block_number = u64_block_number - STORED_BLOCK_HASH_BUFFER;
            Some(BlockHashAndNumber {
                number: BlockNumber(old_block_number),
                hash: block_hashes[usize::try_from(old_block_number).unwrap()],
            })
        };

        // Execute the block.
        let (txs_with_exec, state_maps) =
            execute_block(state, &block_context, prev_block_hash_and_number, txs_for_block);

        // Create a blob, with the previous block hash and proposal commitment.
        let (blob_parameters, block_hash_components, proposal_commitment) = make_blob_parameters(
            &block_context,
            txs_with_exec,
            &state_maps,
            (prev_block_hash, prev_proposal_commitment),
        )
        .await;

        // Commit the block and compute block hash for the next block.
        (prev_block_hash, prev_state_roots, committer_storage) = compute_block_commitments(
            committer_storage,
            &prev_state_roots,
            &state_maps,
            block_hash_components,
            prev_block_hash,
        )
        .await;

        // Update the previous proposal commitment for the next block.
        prev_proposal_commitment = proposal_commitment;

        // Update block hash list.
        assert_eq!(block_hashes.len(), *block_number);
        block_hashes.push(prev_block_hash);

        // Push the new blob.
        blobs.push(
            AerospikeBlob::from_blob_parameters_and_class_manager(
                blob_parameters,
                shared_class_manager.clone(),
            )
            .await
            .unwrap(),
        );
    }
    blobs
}

/// Generates a fixed set of blob data, and one preconfirmed block, with a deterministic list of
/// transactions.
async fn make_data() -> (Vec<AerospikeBlob>, CendeWritePreconfirmedBlock) {
    let (class_manager, transactions) = make_txs();
    let shared_class_manager = Arc::new(class_manager);
    let mut state = DictStateReader::default();

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

// =====================
// Test
// =====================

#[tokio::test]
async fn test_make_data() {
    let (blobs, preconfirmed_block) = make_data().await;
    expect_file![CHAIN_INFO_PATH].assert_eq(
        &serde_json::to_string_pretty(&OsChainInfo::from(&*CHAIN_INFO).to_hex_hashmap()).unwrap(),
    );
    expect_file![BLOB_LIST_PATH].assert_eq(&serde_json::to_string_pretty(&blobs).unwrap());
    expect_file![PRECONFIRMED_BLOCK_PATH]
        .assert_eq(&serde_json::to_string_pretty(&preconfirmed_block).unwrap());
}
