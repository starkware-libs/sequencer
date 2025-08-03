#![allow(dead_code)]
use std::collections::HashMap;
use std::sync::LazyLock;

use blockifier::abi::constants::STORED_BLOCK_HASH_BUFFER;
use blockifier::blockifier_versioned_constants::VersionedConstants;
use blockifier::bouncer::BouncerConfig;
use blockifier::context::{BlockContext, ChainInfo, FeeTokenAddresses};
use blockifier::transaction::transaction_execution::Transaction as BlockifierTransaction;
use starknet_api::block::{BlockHash, BlockInfo, BlockNumber};
use starknet_api::contract_class::ContractClass;
use starknet_api::core::{CompiledClassHash, ContractAddress, Nonce};
use starknet_api::executable_transaction::{
    AccountTransaction,
    DeclareTransaction,
    DeployAccountTransaction,
    InvokeTransaction,
    Transaction as StarknetApiTransaction,
};
use starknet_api::state::SierraContractClass;
use starknet_api::test_utils::{NonceManager, CHAIN_ID_FOR_TESTS};
use starknet_os::io::os_input::{
    OsBlockInput,
    OsChainInfo,
    OsHints,
    OsHintsConfig,
    StarknetOsInput,
};
use starknet_os::io::os_output::StarknetOsRunnerOutput;
use starknet_os::runner::{run_os_stateless, DEFAULT_OS_LAYOUT};
use starknet_patricia_storage::map_storage::BorrowedMapStorage;
use starknet_types_core::felt::Felt;

use crate::initial_state::{
    create_default_initial_state_data,
    get_deploy_fee_token_tx_and_address,
    get_initial_deploy_account_tx,
    InitialState,
    InitialStateData,
    OsExecutionContracts,
};
use crate::state_trait::FlowTestState;
use crate::utils::{
    commit_state_diff,
    create_cached_state_input_and_commitment_infos,
    create_committer_state_diff,
    execute_transactions,
    maybe_dummy_block_hash_and_number,
    CommitmentOutput,
    ExecutionOutput,
};

/// The STRK fee token address that was deployed when initializing the default initial state.
pub(crate) static STRK_FEE_TOKEN_ADDRESS: LazyLock<ContractAddress> =
    LazyLock::new(|| get_deploy_fee_token_tx_and_address(Nonce::default()).1);

/// The address of a funded account that is able to pay fees for transactions.
/// This address was initialized when creating the default initial state.
pub(crate) static FUNDED_ACCOUNT_ADDRESS: LazyLock<ContractAddress> =
    LazyLock::new(|| get_initial_deploy_account_tx().contract_address);

/// Manages the execution of flow tests by maintaining the initial state and transactions.
pub(crate) struct TestManager<S: FlowTestState> {
    pub(crate) initial_state: InitialState<S>,
    pub(crate) execution_contracts: OsExecutionContracts,

    per_block_transactions: Vec<Vec<BlockifierTransaction>>,
}

impl<S: FlowTestState> TestManager<S> {
    /// Creates a new `TestManager` with the provided initial state data.
    pub(crate) fn new_with_initial_state_data(initial_state_data: InitialStateData<S>) -> Self {
        Self {
            initial_state: initial_state_data.initial_state,
            execution_contracts: initial_state_data.execution_contracts,
            per_block_transactions: vec![vec![]],
        }
    }

    /// Creates a new `TestManager` with the default initial state.
    pub(crate) async fn new_with_default_initial_state() -> (Self, NonceManager) {
        let (default_initial_state_data, nonce_manager) =
            create_default_initial_state_data::<S>().await;
        (Self::new_with_initial_state_data(default_initial_state_data), nonce_manager)
    }

    /// Advances the manager to the next block when adding new transactions.
    pub(crate) fn move_to_next_block(&mut self) {
        self.per_block_transactions.push(vec![]);
    }

    fn last_block_txs_mut(&mut self) -> &mut Vec<BlockifierTransaction> {
        self.per_block_transactions
            .last_mut()
            .expect("Always initialized with at least one tx list (at least one block).")
    }

    /// Adds a Cairo 1 declare transaction and updates the execution contracts accordingly.
    pub(crate) fn add_cairo1_declare_tx(
        &mut self,
        tx: DeclareTransaction,
        sierra: &SierraContractClass,
    ) {
        let ContractClass::V1((casm, _sierra_version)) = tx.class_info.contract_class.clone()
        else {
            panic!("Expected a V1 contract class");
        };
        self.last_block_txs_mut().push(BlockifierTransaction::new_for_sequencing(
            StarknetApiTransaction::Account(AccountTransaction::Declare(tx)),
        ));

        self.execution_contracts
            .declared_class_hash_to_component_hashes
            .insert(sierra.calculate_class_hash(), sierra.get_component_hashes());
        let compiled_class_hash = CompiledClassHash(casm.compiled_class_hash());
        self.execution_contracts
            .executed_contracts
            .contracts
            .insert(compiled_class_hash, casm.clone());
    }

    pub(crate) fn add_invoke_tx(&mut self, tx: InvokeTransaction) {
        self.last_block_txs_mut().push(BlockifierTransaction::new_for_sequencing(
            StarknetApiTransaction::Account(AccountTransaction::Invoke(tx)),
        ));
    }

    pub(crate) fn add_cairo0_declare_tx(
        &mut self,
        tx: DeclareTransaction,
        compiled_class_hash: CompiledClassHash,
    ) {
        let ContractClass::V0(class) = tx.class_info.contract_class.clone() else {
            panic!("Expected a V0 contract class");
        };
        self.last_block_txs_mut().push(BlockifierTransaction::new_for_sequencing(
            StarknetApiTransaction::Account(AccountTransaction::Declare(tx)),
        ));
        self.execution_contracts
            .executed_contracts
            .deprecated_contracts
            .insert(compiled_class_hash, class);
    }

    pub(crate) fn add_deploy_account_tx(&mut self, tx: DeployAccountTransaction) {
        self.last_block_txs_mut().push(BlockifierTransaction::new_for_sequencing(
            StarknetApiTransaction::Account(AccountTransaction::DeployAccount(tx)),
        ));
    }

    /// Executes the test using default block contexts, starting from the given block number.
    pub(crate) async fn execute_test_with_default_block_contexts(
        self,
        initial_block_number: u64,
    ) -> StarknetOsRunnerOutput {
        let n_blocks = self.per_block_transactions.len();
        let block_contexts: Vec<BlockContext> = (0..n_blocks)
            .map(|i| {
                block_context_for_flow_tests(BlockNumber(
                    initial_block_number + u64::try_from(i).unwrap(),
                ))
            })
            .collect();
        self.execute_test_with_block_contexts(block_contexts).await
    }

    /// Executes the test using the provided block contexts.
    /// Panics if the number of contexts does not match the number of blocks.
    pub(crate) async fn execute_test_with_block_contexts(
        self,
        block_contexts: Vec<BlockContext>,
    ) -> StarknetOsRunnerOutput {
        assert_eq!(
            block_contexts.len(),
            self.per_block_transactions.len(),
            "Number of block contexts must match number of transaction blocks."
        );
        self.execute_flow_test(block_contexts).await
    }

    // TODO(Nimrod): Add unit tests for the division.
    /// Divides the current transactions into the specified number of blocks.
    /// Panics if there is not exactly one block to divide.
    pub(crate) fn divide_transactions_into_n_blocks(&mut self, n_blocks: usize) {
        assert!(n_blocks > 0, "Nonzero number of blocks expected.");
        assert_eq!(
            self.per_block_transactions.len(),
            1,
            "There should be only one block of transactions to divide."
        );
        let txs = self.per_block_transactions.pop().unwrap();
        let minimal_txs_per_block = txs.len() / n_blocks;
        let remainder = txs.len() % n_blocks;
        let mut txs_per_block = Vec::with_capacity(n_blocks);
        let mut start = 0;
        let mut end = minimal_txs_per_block;
        for i in 0..n_blocks {
            if i < remainder {
                end += 1;
            }
            txs_per_block.push(txs[start..end].to_vec());
            start = end;
            end += minimal_txs_per_block;
        }
        self.per_block_transactions = txs_per_block;
        assert_eq!(self.per_block_transactions.len(), n_blocks);
    }

    // Private method which executes the flow test.
    async fn execute_flow_test(
        mut self,
        block_contexts: Vec<BlockContext>,
    ) -> StarknetOsRunnerOutput {
        let per_block_txs = self.per_block_transactions;
        let mut os_block_inputs = vec![];
        let mut cached_state_inputs = vec![];
        let mut state = self.initial_state.updatable_state;
        let mut map_storage =
            BorrowedMapStorage { storage: &mut self.initial_state.commitment_storage };
        assert_eq!(per_block_txs.len(), block_contexts.len());
        // Commitment output is updated after each block.
        let mut previous_commitment = CommitmentOutput {
            contracts_trie_root_hash: self.initial_state.contracts_trie_root_hash,
            classes_trie_root_hash: self.initial_state.classes_trie_root_hash,
        };
        for (block_txs, block_context) in per_block_txs.into_iter().zip(block_contexts.into_iter())
        {
            // Clone the block info for later use.
            let block_info = block_context.block_info().clone();
            // Execute the transactions.
            let ExecutionOutput { execution_outputs, block_summary, mut final_state } =
                execute_transactions(state, &block_txs, block_context);
            let extended_state_diff = final_state.cache.borrow().extended_state_diff();
            // Update the wrapped state.
            let state_diff = final_state.to_state_diff().unwrap();
            state = final_state.state;
            state.apply_writes(&state_diff.state_maps, &final_state.class_hash_to_class.borrow());
            // Commit the state diff.
            let committer_state_diff = create_committer_state_diff(block_summary.state_diff);
            let new_commitment = commit_state_diff(
                &mut map_storage,
                previous_commitment.contracts_trie_root_hash,
                previous_commitment.classes_trie_root_hash,
                committer_state_diff,
            )
            .await;

            // Prepare the OS input.
            let (cached_state_input, commitment_infos) =
                create_cached_state_input_and_commitment_infos(
                    &previous_commitment,
                    &new_commitment,
                    &mut map_storage,
                    &extended_state_diff,
                );
            let tx_execution_infos = execution_outputs
                .into_iter()
                .map(|(execution_info, _)| execution_info.into())
                .collect();
            // TODO(Nimrod): Remove dummy block hashes once the OS verifies them.
            let old_block_number_and_hash =
                maybe_dummy_block_hash_and_number(block_info.block_number);
            let new_block_hash =
                BlockHash((block_info.block_number.0 + STORED_BLOCK_HASH_BUFFER).into());
            let prev_block_hash = BlockHash(new_block_hash.0 - Felt::ONE);
            let class_hashes_to_migrate = HashMap::new();
            let os_block_input = OsBlockInput {
                contract_state_commitment_info: commitment_infos.contracts_trie_commitment_info,
                contract_class_commitment_info: commitment_infos.classes_trie_commitment_info,
                address_to_storage_commitment_info: commitment_infos.storage_tries_commitment_infos,
                transactions: block_txs.into_iter().map(Into::into).collect(),
                tx_execution_infos,
                declared_class_hash_to_component_hashes: self
                    .execution_contracts
                    .declared_class_hash_to_component_hashes
                    .clone(),
                prev_block_hash,
                new_block_hash,
                block_info,
                old_block_number_and_hash,
                class_hashes_to_migrate,
            };
            os_block_inputs.push(os_block_input);
            cached_state_inputs.push(cached_state_input);
            previous_commitment = new_commitment;
        }
        let starknet_os_input = StarknetOsInput {
            os_block_inputs,
            cached_state_inputs,
            deprecated_compiled_classes: self
                .execution_contracts
                .executed_contracts
                .deprecated_contracts
                .into_iter()
                .collect(),
            compiled_classes: self
                .execution_contracts
                .executed_contracts
                .contracts
                .into_iter()
                .collect(),
        };
        let chain_info = OsChainInfo {
            chain_id: CHAIN_ID_FOR_TESTS.clone(),
            strk_fee_token_address: *STRK_FEE_TOKEN_ADDRESS,
        };
        let os_hints_config = OsHintsConfig { chain_info, ..Default::default() };
        let os_hints = OsHints { os_input: starknet_os_input, os_hints_config };
        let layout = DEFAULT_OS_LAYOUT;
        run_os_stateless(layout, os_hints).unwrap()
    }
}

/// Returns a BlockContext of the given block number with the with the STRK fee token address that
/// was set in the default initial state.
pub fn block_context_for_flow_tests(block_number: BlockNumber) -> BlockContext {
    let fee_token_addresses = FeeTokenAddresses {
        strk_fee_token_address: *STRK_FEE_TOKEN_ADDRESS,
        eth_fee_token_address: ContractAddress::default(),
    };
    BlockContext::new(
        BlockInfo { block_number, ..BlockInfo::create_for_testing() },
        ChainInfo { fee_token_addresses, ..Default::default() },
        VersionedConstants::create_for_testing(),
        BouncerConfig::max(),
    )
}
