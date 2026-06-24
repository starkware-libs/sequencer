use std::path::PathBuf;
use std::sync::{Arc, LazyLock};
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
use apollo_consensus::types::ProposalCommitment;
use apollo_consensus_orchestrator::cende::{
    AerospikeBlob,
    BlobParameters,
    InternalTransactionWithReceipt,
};
use apollo_consensus_orchestrator::dynamic_gas_price::FeeProposalInfo;
use apollo_consensus_orchestrator::fee_market::FeeMarketInfo;
use apollo_infra_utils::compile_time_cargo_manifest_dir;
use blockifier::abi::constants::STORED_BLOCK_HASH_BUFFER;
use blockifier::blockifier::config::TransactionExecutorConfig;
use blockifier::blockifier::transaction_executor::TransactionExecutor;
use blockifier::blockifier_versioned_constants::VersionedConstants;
use blockifier::bouncer::{BouncerConfig, BouncerWeights, CasmHashComputationData};
use blockifier::context::{BlockContext, ChainInfo, FeeTokenAddresses};
use blockifier::state::cached_state::{CachedState, CommitmentStateDiff, StateMaps};
use blockifier::state::state_api::UpdatableState;
use blockifier::test_utils::contracts::FeatureContractTrait;
use blockifier::test_utils::dict_state_reader::DictStateReader;
use blockifier::transaction::account_transaction::AccountTransaction as BlockifierAccountTx;
use blockifier::transaction::transaction_execution::Transaction as BlockifierTx;
use blockifier::transaction::transactions::ExecutableTransaction;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::calldata::create_multicall_calldata;
use blockifier_test_utils::contracts::FeatureContract;
use expect_test::{expect, expect_file, Expect};
use google_cloud_storage::client::{Client, ClientConfig};
use google_cloud_storage::http::error::ErrorResponse;
use google_cloud_storage::http::objects::download::Range;
use google_cloud_storage::http::objects::get::GetObjectRequest;
use google_cloud_storage::http::objects::upload::{Media, UploadObjectRequest, UploadType};
use google_cloud_storage::http::Error as GcsError;
use mockall::predicate::eq;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::block::{BlockHash, BlockHashAndNumber, BlockInfo, BlockNumber, BlockTimestamp};
use starknet_api::block_hash::block_hash_calculator::{
    calculate_block_commitments,
    calculate_block_hash,
    PartialBlockHash,
    PartialBlockHashComponents,
    TransactionHashingData,
};
use starknet_api::consensus_transaction::InternalConsensusTransaction;
use starknet_api::contract_class::compiled_class_hash::HashVersion;
use starknet_api::core::{
    calculate_contract_address,
    ChainId,
    ContractAddress,
    Nonce,
    OsChainInfo,
};
use starknet_api::data_availability::{DataAvailabilityMode, L1DataAvailabilityMode};
use starknet_api::executable_transaction::{
    AccountTransaction as ExecutableAccountTx,
    DeclareTransaction as ExecutableDeclareTx,
    DeployAccountTransaction as ExecutableDeployAccountTx,
    InvokeTransaction as ExecutableInvokeTx,
    Transaction as ExecutableTx,
};
use starknet_api::execution_resources::GasAmount;
use starknet_api::hash::StateRoots;
use starknet_api::rpc_transaction::{
    InternalRpcDeclareTransactionV3,
    InternalRpcDeployAccountTransaction,
    InternalRpcInvokeTransactionV3,
    InternalRpcTransaction,
    InternalRpcTransactionWithoutTxHash,
    RpcDeployAccountTransaction,
    RpcDeployAccountTransactionV3,
};
use starknet_api::state::ThinStateDiff;
use starknet_api::test_utils::{
    NonceManager,
    DEFAULT_STRK_L1_DATA_GAS_PRICE,
    DEFAULT_STRK_L1_GAS_PRICE,
    DEFAULT_STRK_L2_GAS_PRICE,
    TEST_SEQUENCER_ADDRESS,
};
use starknet_api::transaction::fields::{
    AccountDeploymentData,
    AllResourceBounds,
    Calldata,
    ContractAddressSalt,
    PaymasterData,
    ProofFacts,
    ResourceBounds,
    Tip,
    TransactionSignature,
};
use starknet_api::transaction::{
    CalculateContractAddress,
    DeclareTransaction,
    DeployAccountTransaction,
    InvokeTransaction,
    TransactionHash,
    TransactionHasher,
    TransactionOffsetInBlock,
    TransactionVersion,
};
use starknet_api::{calldata, contract_address};
use starknet_committer::block_committer::input::StateDiff;
use starknet_committer::db::facts_db::FactsDb;
use starknet_committer::db::forest_trait::StorageInitializer;
use starknet_core::crypto::ecdsa_sign;
use starknet_crypto::get_public_key;
use starknet_patricia_storage::map_storage::MapStorage;
use starknet_transaction_prover::running::committer_utils::commit_state_diff;
use starknet_types_core::felt::Felt;

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

const EXPECTED_OPERATOR_ADDRESS: Expect =
    expect!["0x059bbf19eb197eb2e5685d4340720ddd7c1124795c736d6c7f2dcf78cd17690a"];
const EXPECTED_FEE_TOKEN_ADDRESS: Expect =
    expect!["0x05945fa70ed44e5f6e5b56c59763dd3d6a8725ebce6f7448b0452f7cd6085056"];
static OPERATOR_ADDRESS: LazyLock<ContractAddress> =
    LazyLock::new(|| contract_address!(EXPECTED_OPERATOR_ADDRESS.data));
static FEE_TOKEN_ADDRESS: LazyLock<ContractAddress> =
    LazyLock::new(|| contract_address!(EXPECTED_FEE_TOKEN_ADDRESS.data));

static NON_TRIVIAL_RESOURCE_BOUNDS: LazyLock<AllResourceBounds> =
    LazyLock::new(|| AllResourceBounds {
        l1_gas: ResourceBounds {
            max_amount: GasAmount(100_000_000),
            max_price_per_unit: DEFAULT_STRK_L1_GAS_PRICE.into(),
        },
        l2_gas: ResourceBounds {
            max_amount: GasAmount(100_000_000_000_000_000),
            max_price_per_unit: DEFAULT_STRK_L2_GAS_PRICE.into(),
        },
        l1_data_gas: ResourceBounds {
            max_amount: GasAmount(100_000),
            max_price_per_unit: DEFAULT_STRK_L1_DATA_GAS_PRICE.into(),
        },
    });

struct TxData {
    executable: ExecutableAccountTx,
    internal: InternalConsensusTransaction,
    should_revert: bool,
}

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

struct BlockData {
    block_context: BlockContext,
    transactions_with_receipts: Vec<InternalTransactionWithReceipt>,
    partial_block_hash_components: PartialBlockHashComponents,
    parent_partial_block_hash_components: Option<PartialBlockHashComponents>,
    block_hash: BlockHash,
    parent_block_hash: BlockHash,
    state_maps: StateMaps,
    state_roots: StateRoots,
}

impl From<BlockData> for BlobParameters {
    /// If this is not the first block, also sets the parent proposal commitment and populates the
    /// recent block hashes only with the hash of the previous block.
    fn from(block: BlockData) -> Self {
        let BlockData {
            block_context,
            transactions_with_receipts,
            partial_block_hash_components,
            parent_partial_block_hash_components,
            parent_block_hash,
            state_maps,
            ..
        } = block;
        let commitment_state_diff = CommitmentStateDiff::from(state_maps);
        let state_diff = ThinStateDiff::from(commitment_state_diff.clone());
        let block_info = block_context.block_info().clone();
        let proposal_commitment = ProposalCommitment(
            PartialBlockHash::from_partial_block_hash_components(&partial_block_hash_components)
                .unwrap()
                .0,
        );

        let (recent_block_hashes, parent_proposal_commitment) = if block_info.block_number.0 > 0 {
            (
                vec![BlockHashAndNumber {
                    number: BlockNumber(block_info.block_number.0 - 1),
                    hash: parent_block_hash,
                }],
                Some(ProposalCommitment(
                    PartialBlockHash::from_partial_block_hash_components(
                        &parent_partial_block_hash_components.unwrap(),
                    )
                    .unwrap()
                    .0,
                )),
            )
        } else {
            (vec![], None)
        };

        Self {
            block_info,
            state_diff,
            compressed_state_diff: Some(commitment_state_diff),
            transactions_with_execution_infos: transactions_with_receipts,
            bouncer_weights: BouncerWeights::default(),
            fee_market_info: FeeMarketInfo::default(),
            fee_proposal_info: FeeProposalInfo::default(),
            casm_hash_computation_data_sierra_gas: CasmHashComputationData::default(),
            casm_hash_computation_data_proving_gas: CasmHashComputationData::default(),
            compiled_class_hashes_for_migration: vec![],
            proposal_commitment,
            parent_proposal_commitment,
            recent_block_hashes,
            #[cfg(feature = "os_input")]
            recent_state_commitment_infos: vec![],
            #[cfg(feature = "os_input")]
            accessed_keys: Default::default(),
            #[cfg(feature = "os_input")]
            initial_reads: Default::default(),
        }
    }
}

struct BlobFactory {
    chain_info: ChainInfo,
    class_manager: MockClassManagerClient,

    // Finalized blocks.
    blocks: Vec<BlockData>,

    // Transactions for the next block.
    next_txs: Vec<TxData>,

    // Context.
    nonce_manager: NonceManager,
    state: DictStateReader,
    committer_storage: FactsDb<MapStorage>,
}

impl BlobFactory {
    const OPERATOR_PRIVATE_KEY: Felt = Felt::THREE;

    pub fn new() -> Self {
        let chain_info = ChainInfo {
            chain_id: CHAIN_ID.clone(),
            fee_token_addresses: FeeTokenAddresses {
                strk_fee_token_address: *FEE_TOKEN_ADDRESS,
                eth_fee_token_address: *FEE_TOKEN_ADDRESS,
            },
            is_l3: false,
        };
        Self {
            chain_info,
            class_manager: MockClassManagerClient::default(),
            blocks: vec![],
            next_txs: vec![],
            nonce_manager: NonceManager::default(),
            state: DictStateReader::default(),
            committer_storage: FactsDb::new(MapStorage::default()),
        }
    }

    /// Executes the unblocked transactions and applies the changes to the state.
    /// Any subsequent transaction added will end up in the next block.
    async fn close_block(&mut self) {
        let block_context = self.next_block_context();
        let block_info = block_context.block_info().clone();
        let block_number = block_info.block_number;

        // Execute the transactions.
        // If the block number is after the block hash buffer, set the previous block hash and
        // number, so they appear in the state diff.
        let old_block_number_and_hash = if block_number.0 < STORED_BLOCK_HASH_BUFFER {
            None
        } else {
            let old_block_number = block_number.0 - STORED_BLOCK_HASH_BUFFER;
            Some(BlockHashAndNumber {
                number: BlockNumber(old_block_number),
                // If we are past the block hash buffer, this should never panic.
                hash: self.blocks[usize::try_from(old_block_number).unwrap()].block_hash,
            })
        };
        let state_clone = self.state.clone();
        let mut executor = TransactionExecutor::pre_process_and_create(
            state_clone,
            block_context.clone(),
            old_block_number_and_hash,
            TransactionExecutorConfig::create_for_testing(false),
        )
        .unwrap();
        let mut transactions_with_receipts = Vec::new();
        // Consume the transactions list (next block starts empty).
        for (tx_index, TxData { executable, internal, should_revert }) in
            std::mem::take(&mut self.next_txs).into_iter().enumerate()
        {
            let (execution_info, _state_changes) = executor
                .execute(&BlockifierTx::new_for_sequencing(ExecutableTx::Account(executable)))
                .unwrap();
            assert_eq!(
                execution_info.is_reverted(),
                should_revert,
                "Transaction at index {tx_index} in block {block_number}: result does not match \
                 expected (should_revert={should_revert}): {execution_info:?}"
            );

            transactions_with_receipts
                .push(InternalTransactionWithReceipt { transaction: internal, execution_info });
        }
        let summary = executor.non_consuming_finalize().unwrap();

        // Apply changes to state and create the multitude of state-diff-like objects required...
        // The [CommitterStateDiff] type is the blockifier representation of the committer's state
        // diff, whereas [StateDiff] is the committer's representation of the state diff.
        let committer_state_diff: CommitmentStateDiff = summary.state_diff.clone();
        let thin_state_diff = ThinStateDiff::from(committer_state_diff.clone());
        let state_diff = StateDiff::from(thin_state_diff.clone());
        let state_maps = StateMaps::from(committer_state_diff.clone());
        let class_mapping = executor.block_state.unwrap().class_hash_to_class.borrow().clone();
        self.state.apply_writes(&state_maps, &class_mapping);

        // Commit the block.
        let prev_state_roots = self.last_finalized_state_roots();
        let state_roots = commit_state_diff(
            &mut self.committer_storage,
            prev_state_roots.contracts_trie_root_hash,
            prev_state_roots.classes_trie_root_hash,
            state_diff,
        )
        .await
        .expect("Failed to commit state diff.");

        // Compute the block hash.
        let transaction_hashing_data: Vec<_> = transactions_with_receipts
            .iter()
            .map(|tx| TransactionHashingData {
                transaction_signature: tx.transaction.tx_signature_for_commitment().unwrap(),
                transaction_output: tx.execution_info.output_for_hashing(),
                transaction_hash: tx.transaction.tx_hash(),
            })
            .collect();
        let (block_header_commitments, _) = calculate_block_commitments(
            &transaction_hashing_data,
            thin_state_diff,
            L1DataAvailabilityMode::from_use_kzg_da(block_info.use_kzg_da),
            &block_info.starknet_version,
        )
        .await;
        let partial_block_hash_components =
            PartialBlockHashComponents::new(&block_info, block_header_commitments);
        let parent_block_hash = self.last_finalized_block_hash();
        let parent_partial_block_hash_components =
            self.last_finalized_partial_block_hash_components();
        let block_hash = calculate_block_hash(
            &partial_block_hash_components,
            state_roots.global_root(),
            parent_block_hash,
        )
        .unwrap();

        // Create and push block data.
        self.blocks.push(BlockData {
            block_context,
            transactions_with_receipts,
            partial_block_hash_components,
            parent_partial_block_hash_components,
            block_hash,
            parent_block_hash,
            state_maps,
            state_roots,
        });
    }

    /// Creates blobs for all finalized blocks, and a preconfirmed block with the remaining txs that
    /// were not included in a block. See [Self::close_block] for details on how to close a block.
    async fn finalize(self) -> (Vec<AerospikeBlob>, CendeWritePreconfirmedBlock) {
        let preconfirmed_block_context = self.next_block_context();
        let Self { blocks, class_manager, next_txs, state, .. } = self;
        let mut blobs = vec![];
        let shared_class_manager = Arc::new(class_manager);

        // For the last block, create a preconfirmed block.
        let preconfirmed_block = Self::make_preconfirmed_block_from_remaining_txs(
            preconfirmed_block_context,
            next_txs,
            state,
        );

        for block in blocks.into_iter() {
            blobs.push(
                AerospikeBlob::from_blob_parameters_and_class_manager(
                    block.into(),
                    shared_class_manager.clone(),
                )
                .await
                .unwrap(),
            );
        }

        (blobs, preconfirmed_block)
    }

    fn last_finalized_block_hash(&self) -> BlockHash {
        self.blocks.last().map(|block| block.block_hash).unwrap_or(BlockHash::GENESIS_PARENT_HASH)
    }

    fn last_finalized_partial_block_hash_components(&self) -> Option<PartialBlockHashComponents> {
        self.blocks.last().map(|block| block.partial_block_hash_components.clone())
    }

    fn last_finalized_state_roots(&self) -> StateRoots {
        self.blocks.last().map(|block| block.state_roots).unwrap_or(StateRoots::EMPTY)
    }

    // =====================
    // Tx generation
    // =====================

    fn sign_tx(tx_hash: TransactionHash) -> TransactionSignature {
        let sig = ecdsa_sign(&Self::OPERATOR_PRIVATE_KEY, &tx_hash.0).unwrap();
        TransactionSignature(Arc::new(vec![sig.r, sig.s]))
    }

    /// If the sender address is None, create a bootstrap declare tx.
    /// Otherwise, create a regular declare tx (with fees).
    fn make_declare_tx(&mut self, contract: FeatureContract, sender: Option<ContractAddress>) {
        let (bootstrap_mode, sender_address, resource_bounds, nonce) = match sender {
            None => (
                true,
                ExecutableDeclareTx::bootstrap_address(),
                AllResourceBounds::new_unlimited_gas_no_fee_enforcement(),
                Nonce::default(),
            ),
            Some(sender_address) => (
                false,
                sender_address,
                *NON_TRIVIAL_RESOURCE_BOUNDS,
                self.nonce_manager.next(sender_address),
            ),
        };
        let sierra = contract.get_sierra();
        let class_hash = sierra.calculate_class_hash();
        let compiled_class_hash = contract.get_compiled_class_hash(&HashVersion::V2);

        // Create internal tx.
        let mut internal_declare_without_hash = InternalRpcDeclareTransactionV3 {
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
        // If not bootrap mode, sign the tx.
        let signature =
            if !bootstrap_mode { Self::sign_tx(tx_hash) } else { TransactionSignature::default() };
        internal_declare_without_hash.signature = signature;
        let internal_tx = InternalConsensusTransaction::RpcTransaction(InternalRpcTransaction {
            tx: InternalRpcTransactionWithoutTxHash::Declare(internal_declare_without_hash.clone()),
            tx_hash,
        });

        // Create executable tx.
        let executable = ExecutableDeclareTx::create(
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
        self.next_txs.push(TxData {
            executable: executable.into(),
            internal: internal_tx,
            should_revert: false,
        });
    }

    fn make_free_deploy_account_tx(&mut self, account: FeatureContract) -> ContractAddress {
        let class_hash = account.get_sierra().calculate_class_hash();
        let public_key = get_public_key(&Self::OPERATOR_PRIVATE_KEY);
        let constructor_calldata = calldata![public_key];
        let contract_address_salt = ContractAddressSalt::default();
        // Build with placeholder signature to compute the hash (signature excluded from hash).
        let rpc_tx_unsigned = RpcDeployAccountTransactionV3 {
            signature: TransactionSignature::default(),
            resource_bounds: AllResourceBounds::new_unlimited_gas_no_fee_enforcement(),
            tip: Tip::default(),
            contract_address_salt,
            class_hash,
            constructor_calldata: constructor_calldata.clone(),
            nonce: Nonce::default(),
            nonce_data_availability_mode: DataAvailabilityMode::L1,
            fee_data_availability_mode: DataAvailabilityMode::L1,
            paymaster_data: PaymasterData::default(),
        };
        let contract_address = rpc_tx_unsigned.calculate_contract_address().unwrap();
        let without_hash_unsigned = InternalRpcTransactionWithoutTxHash::DeployAccount(
            InternalRpcDeployAccountTransaction {
                tx: RpcDeployAccountTransaction::V3(rpc_tx_unsigned.clone()),
                contract_address,
            },
        );
        let tx_hash = without_hash_unsigned.calculate_transaction_hash(&CHAIN_ID).unwrap();
        let signature = Self::sign_tx(tx_hash);

        // Bump nonce for next txs.
        self.nonce_manager.next(contract_address);

        let mut rpc_tx_signed = rpc_tx_unsigned;
        rpc_tx_signed.signature = signature;
        let without_hash = InternalRpcTransactionWithoutTxHash::DeployAccount(
            InternalRpcDeployAccountTransaction {
                tx: RpcDeployAccountTransaction::V3(rpc_tx_signed.clone()),
                contract_address,
            },
        );

        let executable = ExecutableDeployAccountTx::create(
            DeployAccountTransaction::V3(rpc_tx_signed.into()),
            &CHAIN_ID,
        )
        .unwrap();
        let internal = InternalConsensusTransaction::RpcTransaction(InternalRpcTransaction {
            tx: without_hash,
            tx_hash,
        });
        self.next_txs.push(TxData {
            executable: executable.into(),
            internal,
            should_revert: false,
        });
        contract_address
    }

    fn make_operator_invoke_tx(
        &mut self,
        address: ContractAddress,
        function_name: &str,
        calldata: &[Felt],
        with_fee_charge: bool,
        should_revert: bool,
    ) {
        let nonce = self.nonce_manager.next(*OPERATOR_ADDRESS);
        let resource_bounds = if with_fee_charge {
            *NON_TRIVIAL_RESOURCE_BOUNDS
        } else {
            AllResourceBounds::new_unlimited_gas_no_fee_enforcement()
        };
        let calldata =
            Calldata(create_multicall_calldata(&[(address, function_name, calldata)]).into());
        let rpc_tx_unsigned = InternalRpcInvokeTransactionV3 {
            sender_address: *OPERATOR_ADDRESS,
            calldata,
            signature: TransactionSignature::default(),
            resource_bounds,
            tip: Tip::default(),
            nonce,
            nonce_data_availability_mode: DataAvailabilityMode::L1,
            fee_data_availability_mode: DataAvailabilityMode::L1,
            account_deployment_data: AccountDeploymentData::default(),
            paymaster_data: PaymasterData::default(),
            proof_facts: ProofFacts::default(),
        };
        let tx_hash = rpc_tx_unsigned
            .calculate_transaction_hash(&CHAIN_ID, &TransactionVersion::THREE)
            .unwrap();
        let signature = Self::sign_tx(tx_hash);
        let mut rpc_tx_signed = rpc_tx_unsigned;
        rpc_tx_signed.signature = signature;
        let without_hash = InternalRpcTransactionWithoutTxHash::Invoke(rpc_tx_signed.clone());
        let executable =
            ExecutableInvokeTx::create(InvokeTransaction::V3(rpc_tx_signed.into()), &CHAIN_ID)
                .unwrap();
        let internal = InternalConsensusTransaction::RpcTransaction(InternalRpcTransaction {
            tx: without_hash,
            tx_hash,
        });
        self.next_txs.push(TxData { executable: executable.into(), internal, should_revert });
    }

    fn make_operator_deploy_tx(
        &mut self,
        contract_to_deploy: FeatureContract,
        constructor_calldata: Calldata,
        with_fee_charge: bool,
    ) -> ContractAddress {
        let class_hash = contract_to_deploy.get_sierra().calculate_class_hash();
        let contract_address_salt = ContractAddressSalt::default();
        let contract_address = calculate_contract_address(
            contract_address_salt,
            class_hash,
            &constructor_calldata,
            *OPERATOR_ADDRESS,
        )
        .unwrap();
        let calldata = [
            vec![*class_hash, contract_address_salt.0, Felt::from(constructor_calldata.0.len())],
            constructor_calldata.0.as_slice().to_vec(),
            vec![false.into()], // Deploy from zero.
        ]
        .concat();
        self.make_operator_invoke_tx(
            *OPERATOR_ADDRESS,
            "deploy_contract",
            &calldata,
            with_fee_charge,
            false, // should not revert
        );
        contract_address
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

    /// Creates a preconfirmed block for the given block. Should be called for the last block only -
    /// no commitment is computed.
    fn make_preconfirmed_block_from_remaining_txs(
        block_context: BlockContext,
        txs: Vec<TxData>,
        mut state: DictStateReader,
    ) -> CendeWritePreconfirmedBlock {
        let block_info = block_context.block_info().clone();
        let mut transactions = vec![];
        let mut transaction_receipts = vec![];
        let mut transaction_state_diffs = vec![];

        for (tx_index, TxData { executable, internal, should_revert }) in
            txs.into_iter().enumerate()
        {
            let tx_hash = match &internal {
                InternalConsensusTransaction::RpcTransaction(tx) => tx.tx_hash,
                InternalConsensusTransaction::L1Handler(_) => {
                    panic!("unexpected L1Handler in test")
                }
            };

            let mut tx_state = CachedState::new(state.clone());
            let execution_info = BlockifierAccountTx::new_for_sequencing(executable)
                .execute(&mut tx_state, &block_context)
                .unwrap();
            assert_eq!(
                execution_info.is_reverted(),
                should_revert,
                "Transaction at index {tx_index}: result does not match expected \
                 (should_revert={should_revert}): {execution_info:?}"
            );

            let state_changes = tx_state.to_state_diff().unwrap();

            let receipt = StarknetClientTransactionReceipt::from((
                tx_hash,
                TransactionOffsetInBlock(tx_index),
                &execution_info,
                None,
            ));
            let mut tx_state_diff = StarknetClientStateDiff::from(state_changes.state_maps);
            // To keep the output deterministic, sort the state diff.
            tx_state_diff.sort();

            transactions.push(CendePreconfirmedTransaction::from(internal));
            transaction_receipts.push(receipt);
            transaction_state_diffs.push(tx_state_diff.0);

            // Update the state for the next tx.
            state = tx_state.state;
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

/// Sorts arrays of HashSet-backed fields that have non-deterministic iteration order.
/// Object keys are already deterministic because serde_json::Value uses BTreeMap.
fn normalize_set_arrays(value: &mut serde_json::Value) {
    const SET_FIELDS: &[&str] =
        &["accessed_blocks", "accessed_contract_addresses", "accessed_storage_keys"];
    match value {
        serde_json::Value::Object(map) => {
            for (key, val) in map.iter_mut() {
                if SET_FIELDS.contains(&key.as_str()) {
                    if let serde_json::Value::Array(arr) = val {
                        arr.sort_by_key(|a| a.to_string());
                    }
                } else {
                    normalize_set_arrays(val);
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr.iter_mut() {
                normalize_set_arrays(item);
            }
        }
        _ => {}
    }
}

fn to_normalized_json(value: &impl serde::Serialize) -> String {
    let mut json_value = serde_json::to_value(value).unwrap();
    normalize_set_arrays(&mut json_value);
    format!("{}\n", serde_json::to_string_pretty(&json_value).unwrap())
}

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
            Err(GcsError::HttpClient(error))
                if error.status() == Some(http::StatusCode::NOT_FOUND) =>
            {
                break;
            }
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
async fn bump_generation_and_store_blob_file(blobs: Vec<AerospikeBlob>, client: &Client) {
    let blobs_json = to_normalized_json(&blobs);
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
    let mut blob_factory = BlobFactory::new();
    let chain_info = OsChainInfo::from(&blob_factory.chain_info).to_hex_map();

    // Create the list of transactions to be included in the blobs.
    // Block closing point is arbitrary, although it is preferable not to close after the last tx
    // (to ensure the preconfirmed block is not empty).
    let erc20_contract = FeatureContract::ERC20(CairoVersion::Cairo1(RunnableCairo1::Casm));
    let account_with_real_validate = FeatureContract::AccountWithRealValidate(RunnableCairo1::Casm);
    // Use the cende-dedicated feature contract so additions to the shared `test_contract`
    // do not churn this regression's `preconfirmed_block.json` and GCS blob.
    let test_contract = FeatureContract::CendeTest(RunnableCairo1::Casm);
    blob_factory.make_declare_tx(erc20_contract, None);
    blob_factory.close_block().await;
    blob_factory.make_declare_tx(account_with_real_validate, None);
    blob_factory.close_block().await;
    let operator_address = blob_factory.make_free_deploy_account_tx(account_with_real_validate);
    EXPECTED_OPERATOR_ADDRESS.assert_eq(&operator_address.to_string());
    blob_factory.close_block().await;
    let token_address = blob_factory.make_operator_deploy_tx(
        erc20_contract,
        calldata![
            Felt::from_bytes_be_slice(b"StarkNet Token"),
            Felt::from_bytes_be_slice(b"STRK"),
            Felt::from(18u8),
            u128::MAX.into(),   // initial supply lsb
            0.into(),           // initial supply msb
            **operator_address, // recipient address
            **operator_address, // permitted minter
            **operator_address, // provisional_governance_admin
            10.into()           // upgrade delay
        ],
        false, // charge fee
    );
    EXPECTED_FEE_TOKEN_ADDRESS.assert_eq(&token_address.to_string());
    blob_factory.close_block().await;
    blob_factory.make_declare_tx(test_contract, Some(*OPERATOR_ADDRESS));
    blob_factory.close_block().await;
    let test_contract_address_0 = blob_factory.make_operator_deploy_tx(
        test_contract,
        calldata![Felt::ZERO, Felt::ZERO],
        true, // charge fee
    );
    blob_factory.close_block().await;
    let test_contract_address_1 = blob_factory.make_operator_deploy_tx(
        test_contract,
        calldata![Felt::ONE, Felt::ONE],
        true, // charge fee
    );
    blob_factory.close_block().await;
    blob_factory.make_operator_invoke_tx(
        test_contract_address_0,
        "test_increment",
        &[Felt::ZERO; 3],
        true,  // charge fee
        false, // should not revert
    );
    blob_factory.close_block().await;
    blob_factory.make_operator_invoke_tx(
        test_contract_address_1,
        "test_storage_read_write",
        &[Felt::ONE, Felt::TWO],
        true,  // charge fee
        false, // should not revert
    );
    blob_factory.close_block().await;
    blob_factory.make_operator_invoke_tx(
        test_contract_address_1,
        "test_storage_write",
        &[Felt::THREE, Felt::ONE],
        true,  // charge fee
        false, // should not revert
    );
    blob_factory.close_block().await;
    blob_factory.make_operator_invoke_tx(
        test_contract_address_0,
        "write_and_revert",
        &[Felt::from(7u8), Felt::ONE],
        true, // charge fee
        true, // should revert
    );
    blob_factory.close_block().await;
    blob_factory.make_operator_invoke_tx(
        test_contract_address_1,
        "test_call_contract",
        &[
            **test_contract_address_0,
            selector_from_name("test_storage_read_write").0,
            Felt::TWO,
            Felt::from(0x1000),
            Felt::from(0x1000),
        ],
        true,  // charge fee
        false, // should not revert
    );
    blob_factory.close_block().await;
    blob_factory.make_operator_invoke_tx(
        test_contract_address_1,
        "write_1",
        &[Felt::TWO],
        true,  // charge fee
        false, // should not revert
    );
    blob_factory.close_block().await;
    blob_factory.make_operator_invoke_tx(
        test_contract_address_0,
        "catch_write_revert_panic",
        &[**test_contract_address_1, Felt::from(0x2000)],
        true,  // charge fee
        false, // should not revert (inner error is caught)
    );

    let (blobs, preconfirmed_block) = blob_factory.finalize().await;
    expect_file![CHAIN_INFO_PATH].assert_eq(&serde_json::to_string_pretty(&chain_info).unwrap());
    expect_file![PRECONFIRMED_BLOCK_PATH].assert_eq(&to_normalized_json(&preconfirmed_block));

    // Upload or download blobs depending on the fix mode.
    let client = gcs_client().await;
    if env::var("UPDATE_EXPECT").is_ok() {
        bump_generation_and_store_blob_file(blobs, &client).await;
    } else {
        let fetched_blobs = fetch_blob_file(&client).await;
        assert_eq!(
            blobs, fetched_blobs,
            "Blobs mismatch. To fix, run the test with UPDATE_EXPECT=1."
        );
    }
}
