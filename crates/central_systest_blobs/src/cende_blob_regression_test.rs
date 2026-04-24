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
use blockifier_test_utils::calldata::create_calldata;
use blockifier_test_utils::contracts::FeatureContract;
use expect_test::{expect, expect_file, Expect};
use mockall::predicate::eq;
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
    DeclareTransaction as ExecutableDeclareTransaction,
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
use starknet_committer::db::facts_db::db::FactsDb;
use starknet_committer::db::forest_trait::StorageInitializer;
use starknet_core::crypto::ecdsa_sign;
use starknet_crypto::get_public_key;
use starknet_patricia_storage::map_storage::MapStorage;
use starknet_transaction_prover::running::committer_utils::{
    commit_state_diff,
    state_maps_to_committer_state_diff,
};
use starknet_types_core::felt::Felt;

const N_TXS_PER_BLOCK: usize = 1;
static CHAIN_ID: LazyLock<ChainId> =
    LazyLock::new(|| ChainId::Other("SN_PREINTEGRATION_SEPOLIA".to_string()));
static CHAIN_INFO: LazyLock<ChainInfo> = LazyLock::new(|| ChainInfo {
    chain_id: CHAIN_ID.clone(),
    fee_token_addresses: FeeTokenAddresses {
        strk_fee_token_address: FEE_TOKEN_ADDRESS.clone(),
        eth_fee_token_address: FEE_TOKEN_ADDRESS.clone(),
    },
    is_l3: false,
});
const OPERATOR_PRIVATE_KEY: Felt = Felt::THREE;

const CHAIN_INFO_PATH: &str = "../resources/chain_info.json";
const BLOB_LIST_PATH: &str = "../resources/blobs.json";
const PRECONFIRMED_BLOCK_PATH: &str = "../resources/preconfirmed_block.json";

type TxPair = (ExecutableAccountTx, InternalConsensusTransaction);

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

const EXPECTED_OPERATOR_ADDRESS: Expect =
    expect!["0x00f99e7cdfbcce0bf14ce17e4c57fd2d12ad1bca5fc8e46a9fbafc36b59a9955"];
const EXPECTED_FEE_TOKEN_ADDRESS: Expect =
    expect!["0x06bd1d71a2fb67a567618584ca31da288dbc2e1a8421e4045e05f52c19bfab83"];
static OPERATOR_ADDRESS: LazyLock<ContractAddress> =
    LazyLock::new(|| contract_address!(EXPECTED_OPERATOR_ADDRESS.data));
static FEE_TOKEN_ADDRESS: LazyLock<ContractAddress> =
    LazyLock::new(|| contract_address!(EXPECTED_FEE_TOKEN_ADDRESS.data));

// =====================
// Tx generation
// =====================

fn sign_tx(tx_hash: TransactionHash, private_key: Felt) -> TransactionSignature {
    let sig = ecdsa_sign(&private_key, &tx_hash.0).unwrap();
    TransactionSignature(Arc::new(vec![sig.r, sig.s]))
}

fn single_multicall_data(
    sender: ContractAddress,
    function_name: &str,
    calldata: &[Felt],
) -> Calldata {
    let single_calldata = create_calldata(sender, function_name, calldata);
    Calldata(Arc::new([vec![Felt::ONE], single_calldata.0.as_slice().to_vec()].concat()))
}

/// If the sender address is None, create a bootstrap declare tx.
/// Otherwise, create a regular declare tx (with fees).
fn make_declare_tx(
    class_manager: &mut MockClassManagerClient,
    contract: FeatureContract,
    sender: Option<ContractAddress>,
    nonce_manager: &mut NonceManager,
) -> TxPair {
    let (bootstrap_mode, sender_address, resource_bounds, nonce) = match sender {
        None => (
            true,
            ExecutableDeclareTransaction::bootstrap_address(),
            AllResourceBounds::new_unlimited_gas_no_fee_enforcement(),
            Nonce::default(),
        ),
        Some(sender_address) => (
            false,
            sender_address,
            *NON_TRIVIAL_RESOURCE_BOUNDS,
            nonce_manager.next(sender_address),
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
    let signature = if !bootstrap_mode {
        sign_tx(tx_hash, OPERATOR_PRIVATE_KEY)
    } else {
        TransactionSignature::default()
    };
    internal_declare_without_hash.signature = signature;
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

fn make_free_deploy_account_tx(
    nonce_manager: &mut NonceManager,
    account: FeatureContract,
) -> (ContractAddress, TxPair) {
    let class_hash = account.get_sierra().calculate_class_hash();
    let public_key = get_public_key(&OPERATOR_PRIVATE_KEY);
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
    let without_hash_unsigned =
        InternalRpcTransactionWithoutTxHash::DeployAccount(InternalRpcDeployAccountTransaction {
            tx: RpcDeployAccountTransaction::V3(rpc_tx_unsigned.clone()),
            contract_address,
        });
    let tx_hash = without_hash_unsigned.calculate_transaction_hash(&CHAIN_ID).unwrap();
    let signature = sign_tx(tx_hash, OPERATOR_PRIVATE_KEY);

    // Bump nonce for next txs.
    nonce_manager.next(contract_address);

    let mut rpc_tx_signed = rpc_tx_unsigned;
    rpc_tx_signed.signature = signature;
    let without_hash =
        InternalRpcTransactionWithoutTxHash::DeployAccount(InternalRpcDeployAccountTransaction {
            tx: RpcDeployAccountTransaction::V3(rpc_tx_signed.clone()),
            contract_address,
        });

    let executable = ExecutableDeployAccountTx::create(
        DeployAccountTransaction::V3(rpc_tx_signed.into()),
        &CHAIN_ID,
    )
    .unwrap();
    let internal = InternalConsensusTransaction::RpcTransaction(InternalRpcTransaction {
        tx: without_hash,
        tx_hash,
    });
    (contract_address, (executable.into(), internal))
}

fn make_operator_deploy_tx(
    contract_to_deploy: FeatureContract,
    constructor_calldata: Calldata,
    nonce_manager: &mut NonceManager,
    with_fee_charge: bool,
) -> (ContractAddress, (ExecutableAccountTx, InternalConsensusTransaction)) {
    let class_hash = contract_to_deploy.get_sierra().calculate_class_hash();
    let contract_address_salt = ContractAddressSalt::default();
    let contract_address = calculate_contract_address(
        contract_address_salt,
        class_hash,
        &constructor_calldata,
        OPERATOR_ADDRESS.clone(),
    )
    .unwrap();
    let nonce = nonce_manager.next(OPERATOR_ADDRESS.clone());
    let resource_bounds = if with_fee_charge {
        NON_TRIVIAL_RESOURCE_BOUNDS.clone()
    } else {
        AllResourceBounds::new_unlimited_gas_no_fee_enforcement()
    };
    let calldata = single_multicall_data(
        OPERATOR_ADDRESS.clone(),
        "deploy_contract",
        &[
            vec![
                *class_hash,
                contract_address_salt.0,
                Felt::try_from(constructor_calldata.0.len()).unwrap(),
            ],
            constructor_calldata.0.as_slice().to_vec(),
            vec![false.into()], // Deploy from zero.
        ]
        .concat(),
    );
    let rpc_tx_unsigned = InternalRpcInvokeTransactionV3 {
        sender_address: OPERATOR_ADDRESS.clone(),
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
    let tx_hash =
        rpc_tx_unsigned.calculate_transaction_hash(&CHAIN_ID, &TransactionVersion::THREE).unwrap();
    let signature = sign_tx(tx_hash, OPERATOR_PRIVATE_KEY);
    let mut rpc_tx_signed = rpc_tx_unsigned;
    rpc_tx_signed.signature = signature;
    let without_hash = InternalRpcTransactionWithoutTxHash::Invoke(rpc_tx_signed.clone());
    let executable =
        ExecutableInvokeTx::create(InvokeTransaction::V3(rpc_tx_signed.into()), &CHAIN_ID).unwrap();
    let internal = InternalConsensusTransaction::RpcTransaction(InternalRpcTransaction {
        tx: without_hash,
        tx_hash,
    });
    (contract_address, (executable.into(), internal))
}

fn make_txs() -> (MockClassManagerClient, Vec<TxPair>) {
    // Create the list of transactions to be included in the blobs:
    // 1. bootstrap declare of an ERC20 contract.
    // 2. bootstrap declare of an account with real validate.
    // 3. deploy account (with zero fees).
    // 4. deploy ERC20 contract from the account (with zero fees), while minting some tokens to the
    //    sender account.
    // (from this point - all txs include non-zero fees, and no more bootstrap declares)
    // 5. declare the test contract.
    // 6. deploy the test contract.
    // 7. deploy another instance of the test contract.
    // TODO(Dori): the rest of the txs.
    // 8. invoke the test contract: something with a state change.
    // 9. invoke the test contract: test syscalls.

    let mut nonce_manager = NonceManager::default();
    let mut class_manager = MockClassManagerClient::new();
    let erc20_contract = FeatureContract::ERC20(CairoVersion::Cairo1(RunnableCairo1::Casm));
    let account_with_real_validate = FeatureContract::AccountWithRealValidate(RunnableCairo1::Casm);
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));

    // Bootstrap declares.
    let erc20_declare_tx =
        make_declare_tx(&mut class_manager, erc20_contract, None, &mut nonce_manager);
    let account_with_real_validate_declare_tx =
        make_declare_tx(&mut class_manager, account_with_real_validate, None, &mut nonce_manager);

    // Free deploy-account.
    let (operator_address, deploy_operator_account_tx) =
        make_free_deploy_account_tx(&mut nonce_manager, account_with_real_validate);
    EXPECTED_OPERATOR_ADDRESS.assert_eq(&operator_address.to_string());

    // Deploy ERC20 contract from the account (with zero fees), while minting some tokens to the
    // sender account.
    let (token_address, deploy_erc20_tx) = make_operator_deploy_tx(
        erc20_contract,
        calldata![
            Felt::from_bytes_be_slice(b"StarkNet Token"),
            Felt::from_bytes_be_slice(b"STRK"),
            Felt::from(18u8),
            u128::MAX.into(),    // initial supply lsb
            0.into(),            // initial supply msb
            ***OPERATOR_ADDRESS, // recipient address
            ***OPERATOR_ADDRESS, // permitted minter
            ***OPERATOR_ADDRESS, // provisional_governance_admin
            10.into()            // upgrade delay
        ],
        &mut nonce_manager,
        false,
    );
    EXPECTED_FEE_TOKEN_ADDRESS.assert_eq(&token_address.to_string());

    // Declare the test contract.
    let test_contract_declare_tx = make_declare_tx(
        &mut class_manager,
        test_contract,
        Some(operator_address),
        &mut nonce_manager,
    );

    // Deploy the test contract, twice.
    let (_test_contract_address_0, deploy_test_contract_tx_0) = make_operator_deploy_tx(
        test_contract,
        calldata![Felt::ZERO, Felt::ZERO],
        &mut nonce_manager,
        true,
    );
    let (_test_contract_address_1, deploy_test_contract_tx_1) = make_operator_deploy_tx(
        test_contract,
        calldata![Felt::ZERO, Felt::ZERO],
        &mut nonce_manager,
        true,
    );

    (
        class_manager,
        vec![
            erc20_declare_tx,
            account_with_real_validate_declare_tx,
            deploy_operator_account_tx,
            deploy_erc20_tx,
            test_contract_declare_tx,
            deploy_test_contract_tx_0,
            deploy_test_contract_tx_1,
        ],
    )
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
    state: &mut DictStateReader,
    block_context: &BlockContext,
    old_block_number_and_hash: Option<BlockHashAndNumber>,
    txs: &[TxPair],
) -> (Vec<InternalTransactionWithReceipt>, StateMaps) {
    let state_clone = state.clone();
    let mut executor = TransactionExecutor::pre_process_and_create(
        state_clone,
        block_context.clone(),
        old_block_number_and_hash,
        TransactionExecutorConfig::create_for_testing(false),
    )
    .unwrap();

    let mut txs_with_exec = Vec::new();

    for (executable, internal) in txs {
        let (execution_info, _state_changes) = executor
            .execute(&BlockifierTx::new_for_sequencing(ExecutableTx::Account(executable.clone())))
            .unwrap();

        txs_with_exec
            .push(InternalTransactionWithReceipt { transaction: internal.clone(), execution_info });
    }

    let summary = executor.non_consuming_finalize().unwrap();
    let final_state_maps = summary.state_diff.into();
    let class_mapping = executor.block_state.unwrap().class_hash_to_class.borrow().clone();
    state.apply_writes(&final_state_maps, &class_mapping);

    (txs_with_exec, final_state_maps)
}

async fn compute_block_hash_components(
    block_info: &BlockInfo,
    state_diff: &ThinStateDiff,
    txs: &[InternalTransactionWithReceipt],
) -> PartialBlockHashComponents {
    let transaction_hashing_data: Vec<_> = txs
        .iter()
        .map(|tx| TransactionHashingData {
            transaction_signature: tx.transaction.tx_signature_for_commitment().unwrap(),
            transaction_output: tx.execution_info.output_for_hashing(),
            transaction_hash: tx.transaction.tx_hash(),
        })
        .collect();
    let l1_da_mode = L1DataAvailabilityMode::default();
    let (block_header_commitments, _) = calculate_block_commitments(
        &transaction_hashing_data,
        state_diff.clone(),
        l1_da_mode,
        &block_info.starknet_version,
    )
    .await;
    PartialBlockHashComponents::new(block_info, block_header_commitments)
}

/// Given previous state and partial components, commits the changes and finalizes the block hash.
/// Returns the block hash, the new state roots and the updated committer storage.
async fn compute_block_commitments(
    committer_storage: MapStorage,
    prev_state_roots: &StateRoots,
    state_maps: &StateMaps,
    block_hash_components: PartialBlockHashComponents,
    prev_block_hash: BlockHash,
) -> (BlockHash, StateRoots, MapStorage) {
    // Commit the state diff.
    let committer_state_diff = state_maps_to_committer_state_diff(state_maps.clone());
    let mut db = FactsDb::new(committer_storage);
    let new_state_roots = commit_state_diff(
        &mut db,
        prev_state_roots.contracts_trie_root_hash,
        prev_state_roots.classes_trie_root_hash,
        committer_state_diff,
    )
    .await
    .expect("Failed to commit state diff.");

    // Compute the block hash.
    let block_hash = calculate_block_hash(
        &block_hash_components,
        new_state_roots.global_root(),
        prev_block_hash,
    )
    .unwrap();
    (block_hash, new_state_roots, db.consume_storage())
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
        let mut tx_state_diff = StarknetClientStateDiff::from(state_changes.state_maps);
        // To keep the output deterministic, sort the state diff.
        tx_state_diff.sort();

        transactions.push(CendePreconfirmedTransaction::from(internal.clone()));
        transaction_receipts.push(Some(receipt));
        transaction_state_diffs.push(Some(tx_state_diff.0));
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
                        arr.sort_by(|a, b| a.to_string().cmp(&b.to_string()));
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

// =====================
// Test
// =====================

#[tokio::test]
async fn test_make_data() {
    let (blobs, preconfirmed_block) = make_data().await;
    expect_file![CHAIN_INFO_PATH].assert_eq(
        &serde_json::to_string_pretty(&OsChainInfo::from(&*CHAIN_INFO).to_hex_hashmap()).unwrap(),
    );
    expect_file![BLOB_LIST_PATH].assert_eq(&to_normalized_json(&blobs));
    expect_file![PRECONFIRMED_BLOCK_PATH].assert_eq(&to_normalized_json(&preconfirmed_block));
}
