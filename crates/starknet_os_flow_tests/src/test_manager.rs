#![allow(dead_code)]

use blockifier::blockifier_versioned_constants::VersionedConstants;
use blockifier::bouncer::BouncerConfig;
use blockifier::context::{BlockContext, ChainInfo, FeeTokenAddresses};
use blockifier::transaction::transaction_execution::Transaction as BlockifierTransaction;
use starknet_api::block::{BlockInfo, BlockNumber};
use starknet_api::contract_class::ContractClass;
use starknet_api::core::{CompiledClassHash, ContractAddress};
use starknet_api::executable_transaction::{
    AccountTransaction,
    DeclareTransaction,
    DeployAccountTransaction,
    InvokeTransaction,
    Transaction as StarknetApiTransaction,
};
use starknet_api::state::SierraContractClass;
use starknet_os::io::os_output::StarknetOsRunnerOutput;

use crate::initial_state::{
    create_default_initial_state_data,
    InitialState,
    InitialStateData,
    OsExecutionContracts,
};
use crate::state_trait::FlowTestState;

// TODO(Nimrod): Replace with actual values.
/// The STRK fee token address that was deployed when initializing the default initial state.
pub(crate) const STRK_FEE_TOKEN_ADDRESS: u128 = 11;

/// The address of a funded account that is able to pay fees for transactions.
/// This address was initialized when creating the default initial state.
pub(crate) const FUNDED_ACCOUNT_ADDRESS: u128 = 12;

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
    pub(crate) async fn new_with_default_initial_state() -> Self {
        let default_initial_state_data = create_default_initial_state_data::<S>().await;
        Self::new_with_initial_state_data(default_initial_state_data)
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

        todo!()
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
}

/// Returns a BlockContext of the given block number with the with the STRK fee token address that
/// was set in the default initial state.
pub fn block_context_for_flow_tests(block_number: BlockNumber) -> BlockContext {
    let fee_token_addresses = FeeTokenAddresses {
        strk_fee_token_address: STRK_FEE_TOKEN_ADDRESS.into(),
        eth_fee_token_address: ContractAddress::default(),
    };
    BlockContext::new(
        BlockInfo { block_number, ..BlockInfo::create_for_testing() },
        ChainInfo { fee_token_addresses, ..Default::default() },
        VersionedConstants::create_for_testing(),
        BouncerConfig::max(),
    )
}
