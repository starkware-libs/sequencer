#![allow(dead_code)]
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use blockifier::abi::constants::STORED_BLOCK_HASH_BUFFER;
use blockifier::blockifier_versioned_constants::VersionedConstants;
use blockifier::bouncer::BouncerConfig;
use blockifier::context::{BlockContext, ChainInfo, FeeTokenAddresses};
use blockifier::state::cached_state::{CommitmentStateDiff, StateMaps};
use blockifier::state::stateful_compression_test_utils::decompress;
use blockifier::test_utils::ALIAS_CONTRACT_ADDRESS;
use blockifier::transaction::objects::TransactionExecutionInfo;
use blockifier::transaction::transaction_execution::Transaction as BlockifierTransaction;
use blockifier_test_utils::contracts::FeatureContract;
use itertools::Itertools;
use starknet_api::abi::abi_utils::get_fee_token_var_address;
use starknet_api::block::{BlockHash, BlockInfo, BlockNumber, PreviousBlockNumber};
use starknet_api::contract_class::compiled_class_hash::{HashVersion, HashableCompiledClass};
use starknet_api::contract_class::ContractClass;
use starknet_api::core::{ChainId, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::executable_transaction::{
    AccountTransaction,
    DeclareTransaction,
    DeployAccountTransaction,
    InvokeTransaction,
    L1HandlerTransaction,
    Transaction as StarknetApiTransaction,
};
use starknet_api::state::{SierraContractClass, StorageKey};
use starknet_api::test_utils::invoke::{invoke_tx, InvokeTxArgs};
use starknet_api::test_utils::{NonceManager, CHAIN_ID_FOR_TESTS};
use starknet_api::transaction::fields::Calldata;
use starknet_api::transaction::MessageToL1;
use starknet_committer::block_committer::input::{IsSubset, StarknetStorageKey, StateDiff};
use starknet_os::io::os_input::{
    OsBlockInput,
    OsChainInfo,
    OsHints,
    OsHintsConfig,
    StarknetOsInput,
};
use starknet_os::io::os_output::{MessageToL2, OsStateDiff, StarknetOsRunnerOutput};
use starknet_os::io::os_output_types::{
    FullOsStateDiff,
    PartialCommitmentOsStateDiff,
    PartialOsStateDiff,
    TryFromOutputIter,
};
use starknet_os::io::test_utils::validate_kzg_segment;
use starknet_os::runner::{run_os_stateless_for_testing, DEFAULT_OS_LAYOUT};
use starknet_patricia::hash::hash_trait::HashOutput;
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
    divide_vec_into_n_parts,
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

#[derive(Default)]
pub(crate) struct TestParameters {
    pub(crate) use_kzg_da: bool,
    pub(crate) full_output: bool,
    pub(crate) messages_to_l1: Vec<MessageToL1>,
    pub(crate) messages_to_l2: Vec<MessageToL2>,
}

pub(crate) struct FlowTestTx {
    tx: BlockifierTransaction,
    expected_revert_reason: Option<String>,
}

/// Manages the execution of flow tests by maintaining the initial state and transactions.
pub(crate) struct TestManager<S: FlowTestState> {
    pub(crate) initial_state: InitialState<S>,
    pub(crate) execution_contracts: OsExecutionContracts,

    per_block_transactions: Vec<Vec<FlowTestTx>>,
}

pub(crate) struct OsTestExpectedValues {
    pub(crate) previous_global_root: HashOutput,
    pub(crate) new_global_root: HashOutput,
    pub(crate) previous_block_number: PreviousBlockNumber,
    pub(crate) new_block_number: BlockNumber,
    pub(crate) config_hash: Felt,
    pub(crate) use_kzg_da: bool,
    pub(crate) full_output: bool,
    pub(crate) messages_to_l1: Vec<MessageToL1>,
    pub(crate) messages_to_l2: Vec<MessageToL2>,
    pub(crate) committed_state_diff: StateDiff,
}

pub(crate) struct OsTestOutput<S: FlowTestState> {
    pub(crate) runner_output: StarknetOsRunnerOutput,
    pub(crate) decompressed_state_diff: StateDiff,
    pub(crate) final_state: S,
    pub(crate) expected_values: OsTestExpectedValues,
}

impl<S: FlowTestState> OsTestOutput<S> {
    pub(crate) fn perform_default_validations(&self) {
        self.perform_validations(true, None);
    }

    pub(crate) fn perform_validations(
        &self,
        perform_global_validations: bool,
        partial_state_diff: Option<&StateDiff>,
    ) {
        if perform_global_validations {
            self.perform_global_validations();
        }
        if let Some(partial_state_diff) = partial_state_diff {
            assert!(partial_state_diff.is_subset(&self.decompressed_state_diff));
        }
    }

    #[track_caller]
    pub(crate) fn assert_account_balance_change(&self, account_address: ContractAddress) {
        assert!(
            self.decompressed_state_diff
                .storage_updates
                .get(&STRK_FEE_TOKEN_ADDRESS)
                .expect("Expect balance changes.")
                .contains_key(&StarknetStorageKey(get_fee_token_var_address(account_address)))
        );
    }

    fn perform_global_validations(&self) {
        // TODO(Dori): Implement global validations for the OS test output.

        // Builtin validations are done in `run_os_stateless_for_testing`.

        let os_output = self
            .runner_output
            .get_os_output()
            .expect("Getting OsOutput from raw OS output should not fail.");

        // Validate state roots.
        assert_eq!(
            os_output.common_os_output.initial_root,
            self.expected_values.previous_global_root.0
        );
        assert_eq!(os_output.common_os_output.final_root, self.expected_values.new_global_root.0);

        // Block numbers.
        assert_eq!(
            os_output.common_os_output.prev_block_number,
            self.expected_values.previous_block_number
        );
        assert_eq!(
            os_output.common_os_output.new_block_number,
            self.expected_values.new_block_number
        );

        // TODO(Dori): Implement block hash validation.

        // Config hash.
        assert_eq!(
            os_output.common_os_output.starknet_os_config_hash,
            self.expected_values.config_hash,
        );

        // Flags.
        assert_eq!(os_output.use_kzg_da(), self.expected_values.use_kzg_da);
        assert_eq!(os_output.full_output(), self.expected_values.full_output);

        // KZG commitment.
        if os_output.use_kzg_da() {
            let OsStateDiff::PartialCommitment(PartialCommitmentOsStateDiff(
                ref partial_commitment,
            )) = os_output.state_diff
            else {
                panic!(
                    "Expected a PartialCommitment state diff when use_kzg_da is true; full_output \
                     should be false."
                );
            };
            validate_kzg_segment(
                self.runner_output.da_segment.as_ref().unwrap(),
                partial_commitment,
            );
        }

        // Messages.
        assert_eq!(os_output.common_os_output.messages_to_l1, self.expected_values.messages_to_l1);
        assert_eq!(os_output.common_os_output.messages_to_l2, self.expected_values.messages_to_l2);

        // State diff.
        // Storage diffs should always be equal, but in full output mode there is extra data in the
        // OS output - any contract address with any change (nonce, class hash or storage) will have
        // both nonce and class hash in the output.
        if os_output.full_output() {
            // Fill in class hashes / nonces for all addresses with changes.
            let mut full_state_diff = self.expected_values.committed_state_diff.clone();
            for address in self
                .decompressed_state_diff
                .address_to_class_hash
                .keys()
                .chain(self.decompressed_state_diff.address_to_nonce.keys())
                .chain(self.decompressed_state_diff.storage_updates.keys())
                .unique()
            {
                // Read the current nonce / class hash from the state.
                let nonce = self.final_state.get_nonce_at(*address).unwrap();
                let class_hash = self.final_state.get_class_hash_at(*address).unwrap();
                full_state_diff.address_to_nonce.insert(*address, nonce);
                full_state_diff.address_to_class_hash.insert(*address, class_hash);
            }
            assert_eq!(self.decompressed_state_diff, full_state_diff);
        } else {
            assert_eq!(self.decompressed_state_diff, self.expected_values.committed_state_diff);
        }
    }
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
    /// Returns the manager and a nonce manager to help keep track of nonces.
    /// Optionally provide an array of extra contracts to declare and deploy - the addresses of
    /// these contracts will be returned as an array of the same length.
    pub(crate) async fn new_with_default_initial_state<const N: usize>(
        extra_contracts: [(FeatureContract, Calldata); N],
    ) -> (Self, NonceManager, [ContractAddress; N]) {
        let (default_initial_state_data, nonce_manager, extra_addresses) =
            create_default_initial_state_data::<S, N>(extra_contracts).await;
        (
            Self::new_with_initial_state_data(default_initial_state_data),
            nonce_manager,
            extra_addresses,
        )
    }

    /// Advances the manager to the next block when adding new transactions.
    pub(crate) fn move_to_next_block(&mut self) {
        self.per_block_transactions.push(vec![]);
    }

    pub(crate) fn total_txs(&self) -> usize {
        self.per_block_transactions.iter().map(|block| block.len()).sum()
    }

    fn last_block_txs_mut(&mut self) -> &mut Vec<FlowTestTx> {
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
        self.last_block_txs_mut().push(FlowTestTx {
            tx: BlockifierTransaction::new_for_sequencing(StarknetApiTransaction::Account(
                AccountTransaction::Declare(tx),
            )),
            expected_revert_reason: None,
        });

        self.execution_contracts
            .declared_class_hash_to_component_hashes
            .insert(sierra.calculate_class_hash(), sierra.get_component_hashes());
        let compiled_class_hash = casm.hash(&HashVersion::V2);
        self.execution_contracts
            .executed_contracts
            .contracts
            .insert(compiled_class_hash, casm.clone());
    }

    pub(crate) fn add_invoke_tx(
        &mut self,
        tx: InvokeTransaction,
        expected_revert_reason: Option<String>,
    ) {
        self.last_block_txs_mut().push(FlowTestTx {
            tx: BlockifierTransaction::new_for_sequencing(StarknetApiTransaction::Account(
                AccountTransaction::Invoke(tx),
            )),
            expected_revert_reason,
        });
    }

    pub(crate) fn add_invoke_tx_from_args(
        &mut self,
        args: InvokeTxArgs,
        chain_id: &ChainId,
        revert_reason: Option<String>,
    ) {
        self.add_invoke_tx(
            InvokeTransaction::create(invoke_tx(args), chain_id).unwrap(),
            revert_reason,
        );
    }

    pub(crate) fn add_cairo0_declare_tx(
        &mut self,
        tx: DeclareTransaction,
        compiled_class_hash: CompiledClassHash,
    ) {
        let ContractClass::V0(class) = tx.class_info.contract_class.clone() else {
            panic!("Expected a V0 contract class");
        };
        self.last_block_txs_mut().push(FlowTestTx {
            tx: BlockifierTransaction::new_for_sequencing(StarknetApiTransaction::Account(
                AccountTransaction::Declare(tx),
            )),
            expected_revert_reason: None,
        });
        self.execution_contracts
            .executed_contracts
            .deprecated_contracts
            .insert(compiled_class_hash, class);
    }

    pub(crate) fn add_deploy_account_tx(&mut self, tx: DeployAccountTransaction) {
        self.last_block_txs_mut().push(FlowTestTx {
            tx: BlockifierTransaction::new_for_sequencing(StarknetApiTransaction::Account(
                AccountTransaction::DeployAccount(tx),
            )),
            expected_revert_reason: None,
        });
    }

    pub(crate) fn add_l1_handler_tx(
        &mut self,
        tx: L1HandlerTransaction,
        expected_revert_reason: Option<String>,
    ) {
        self.last_block_txs_mut().push(FlowTestTx {
            tx: BlockifierTransaction::new_for_sequencing(StarknetApiTransaction::L1Handler(tx)),
            expected_revert_reason,
        });
    }

    /// Executes the test using default block contexts, starting from the given block number.
    pub(crate) async fn execute_test_with_default_block_contexts(
        self,
        test_params: &TestParameters,
    ) -> OsTestOutput<S> {
        let n_blocks = self.per_block_transactions.len();
        let block_contexts: Vec<BlockContext> = (0..n_blocks)
            .map(|i| {
                block_context_for_flow_tests(
                    BlockNumber(self.initial_state.next_block_number.0 + u64::try_from(i).unwrap()),
                    test_params.use_kzg_da,
                )
            })
            .collect();
        self.execute_test_with_block_contexts(block_contexts, test_params).await
    }

    /// Executes the test using the provided block contexts.
    /// Panics if the number of contexts does not match the number of blocks.
    pub(crate) async fn execute_test_with_block_contexts(
        self,
        block_contexts: Vec<BlockContext>,
        test_params: &TestParameters,
    ) -> OsTestOutput<S> {
        assert_eq!(
            block_contexts.len(),
            self.per_block_transactions.len(),
            "Number of block contexts must match number of transaction blocks."
        );
        self.execute_flow_test(block_contexts, test_params).await
    }

    /// Divides the current transactions into the specified number of blocks.
    /// Panics if there is not exactly one block to divide.
    pub(crate) fn divide_transactions_into_n_blocks(&mut self, n_blocks: usize) {
        assert_eq!(
            self.per_block_transactions.len(),
            1,
            "There should be only one block of transactions to divide."
        );
        self.per_block_transactions =
            divide_vec_into_n_parts(self.per_block_transactions.pop().unwrap(), n_blocks);
    }

    /// Verifies all [ChainInfo]s are identical in the given block contexts, and returns an
    /// [OsChainInfo].
    fn verify_chain_infos_and_get_one(block_contexts: &[BlockContext]) -> OsChainInfo {
        let mut chain_info_iter = block_contexts.iter().map(|ctx| ctx.chain_info());
        let first_chain_info =
            chain_info_iter.next().expect("At least one block context is required.");
        assert!(
            chain_info_iter.all(|chain_info| first_chain_info == chain_info),
            "All block contexts must have the same chain info."
        );
        OsChainInfo {
            chain_id: first_chain_info.chain_id.clone(),
            strk_fee_token_address: first_chain_info.fee_token_addresses.strk_fee_token_address,
        }
    }

    /// Verifies all [BlockContext]s have the same `use_kzg_da` flag, and returns it.
    fn verify_kzg_da_flag_and_get(block_contexts: &[BlockContext]) -> bool {
        let mut use_kzg_da_iter = block_contexts.iter().map(|ctx| ctx.block_info().use_kzg_da);
        let first_use_kzg_da =
            use_kzg_da_iter.next().expect("At least one block context is required.");
        assert!(
            use_kzg_da_iter.all(|use_kzg_da| first_use_kzg_da == use_kzg_da),
            "All block contexts must have the same use_kzg_da flag."
        );
        first_use_kzg_da
    }

    /// Verifies all the execution outputs are as expected w.r.t. revert reasons.
    fn verify_execution_outputs(
        block_index: usize,
        revert_reasons: &[Option<String>],
        execution_outputs: &[(TransactionExecutionInfo, StateMaps)],
    ) {
        assert_eq!(revert_reasons.len(), execution_outputs.len());
        for ((i, revert_reason), (execution_info, _)) in
            revert_reasons.iter().enumerate().zip(execution_outputs.iter())
        {
            let preamble = format!("Block {block_index}, transaction {i}:");
            if let Some(revert_reason) = revert_reason {
                let actual_revert_reason =
                    execution_info.revert_error.as_ref().unwrap().to_string();
                assert!(
                    actual_revert_reason.contains(revert_reason),
                    "{preamble} Expected '{revert_reason}' to be in revert \
                     string:\n'{actual_revert_reason}'"
                );
            } else {
                assert!(
                    execution_info.revert_error.is_none(),
                    "{preamble} Expected no revert error, got: {}.",
                    execution_info.revert_error.as_ref().unwrap()
                );
            }
        }
    }

    /// Decompresses the state diff from the OS output using the given OS output, state and alias
    /// keys.
    fn get_decompressed_state_diff(
        runner_output: &StarknetOsRunnerOutput,
        state: &S,
        alias_keys: HashSet<StorageKey>,
    ) -> StateMaps {
        let os_output = runner_output
            .get_os_output()
            .expect("Getting OsOutput from raw OS output should not fail.");
        let os_state_diff_maps = match os_output.state_diff {
            OsStateDiff::Partial(ref partial_os_state_diff) => {
                partial_os_state_diff.as_state_maps()
            }
            OsStateDiff::Full(ref full_os_state_diff) => full_os_state_diff.as_state_maps(),
            // In commitment modes, state diff should be deserialized from the DA segment.
            OsStateDiff::PartialCommitment(_) => {
                let da_segment = runner_output.da_segment.clone().unwrap();
                PartialOsStateDiff::try_from_output_iter(&mut da_segment.into_iter())
                    .unwrap()
                    .as_state_maps()
            }
            OsStateDiff::FullCommitment(_) => {
                let da_segment = runner_output.da_segment.clone().unwrap();
                FullOsStateDiff::try_from_output_iter(&mut da_segment.into_iter())
                    .unwrap()
                    .as_state_maps()
            }
        };
        decompress(&os_state_diff_maps, state, *ALIAS_CONTRACT_ADDRESS, alias_keys)
    }

    // Private method which executes the flow test.
    async fn execute_flow_test(
        self,
        block_contexts: Vec<BlockContext>,
        test_params: &TestParameters,
    ) -> OsTestOutput<S> {
        let per_block_txs = self.per_block_transactions;
        let mut os_block_inputs = vec![];
        let mut cached_state_inputs = vec![];
        let initial_state = self.initial_state.updatable_state.clone();
        let mut state = self.initial_state.updatable_state;
        let mut map_storage = self.initial_state.commitment_storage;
        assert_eq!(per_block_txs.len(), block_contexts.len());
        // Commitment output is updated after each block.
        let mut previous_commitment = CommitmentOutput {
            contracts_trie_root_hash: self.initial_state.contracts_trie_root_hash,
            classes_trie_root_hash: self.initial_state.classes_trie_root_hash,
        };
        let mut entire_state_diff = StateDiff::default();
        let expected_previous_global_root = previous_commitment.global_root();
        let previous_block_number =
            PreviousBlockNumber(block_contexts.first().unwrap().block_info().block_number.prev());
        let new_block_number = block_contexts.last().unwrap().block_info().block_number;
        let chain_info = Self::verify_chain_infos_and_get_one(&block_contexts);
        let use_kzg_da = Self::verify_kzg_da_flag_and_get(&block_contexts);
        assert_eq!(
            use_kzg_da, test_params.use_kzg_da,
            "use_kzg_da flag in block contexts must match the test parameter."
        );
        let mut alias_keys = HashSet::new();
        for ((block_index, block_txs_with_reason), block_context) in
            per_block_txs.into_iter().enumerate().zip(block_contexts.into_iter())
        {
            let (block_txs, revert_reasons): (Vec<_>, Vec<_>) = block_txs_with_reason
                .into_iter()
                .map(|flow_test_tx| (flow_test_tx.tx, flow_test_tx.expected_revert_reason))
                .unzip();
            // Clone the block info for later use.
            let block_info = block_context.block_info().clone();
            // Execute the transactions.
            let ExecutionOutput { execution_outputs, block_summary, mut final_state } =
                execute_transactions(state, &block_txs, block_context);
            Self::verify_execution_outputs(block_index, &revert_reasons, &execution_outputs);
            let extended_state_diff = final_state.cache.borrow().extended_state_diff();
            // Update the wrapped state.
            let state_diff = final_state.to_state_diff().unwrap();
            state = final_state.state;
            alias_keys.extend(state_diff.state_maps.alias_keys());
            state.apply_writes(&state_diff.state_maps, &final_state.class_hash_to_class.borrow());
            // Commit the state diff.
            let committer_state_diff = create_committer_state_diff(block_summary.state_diff);
            entire_state_diff.extend(committer_state_diff.clone());
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
        let expected_new_global_root = previous_commitment.global_root();
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
        let expected_config_hash = chain_info.compute_os_config_hash().unwrap();
        let os_hints_config = OsHintsConfig {
            chain_info,
            use_kzg_da,
            full_output: test_params.full_output,
            ..Default::default()
        };
        let os_hints = OsHints { os_input: starknet_os_input, os_hints_config };
        let layout = DEFAULT_OS_LAYOUT;
        let (os_output, _) = run_os_stateless_for_testing(layout, os_hints).unwrap();
        let decompressed_state_diff = create_committer_state_diff(CommitmentStateDiff::from(
            Self::get_decompressed_state_diff(&os_output, &state, alias_keys),
        ));

        OsTestOutput {
            runner_output: os_output,
            decompressed_state_diff,
            final_state: state,
            expected_values: OsTestExpectedValues {
                previous_global_root: expected_previous_global_root,
                new_global_root: expected_new_global_root,
                previous_block_number,
                new_block_number,
                config_hash: expected_config_hash,
                full_output: test_params.full_output,
                // The OS will not compute a KZG commitment in full output mode.
                use_kzg_da: use_kzg_da && !test_params.full_output,
                messages_to_l1: test_params.messages_to_l1.clone(),
                messages_to_l2: test_params.messages_to_l2.clone(),
                committed_state_diff: initial_state.nontrivial_diff(entire_state_diff),
            },
        }
    }
}

/// Returns a BlockContext of the given block number with the with the STRK fee token address that
/// was set in the default initial state.
pub fn block_context_for_flow_tests(block_number: BlockNumber, use_kzg_da: bool) -> BlockContext {
    let fee_token_addresses = FeeTokenAddresses {
        strk_fee_token_address: *STRK_FEE_TOKEN_ADDRESS,
        // Reuse the same token address for ETH fee token, for ease of testing (only need to fund
        // accounts with one token to send deprecated declares).
        eth_fee_token_address: *STRK_FEE_TOKEN_ADDRESS,
    };
    BlockContext::new(
        BlockInfo { block_number, use_kzg_da, ..BlockInfo::create_for_testing() },
        ChainInfo {
            fee_token_addresses,
            chain_id: CHAIN_ID_FOR_TESTS.clone(),
            ..Default::default()
        },
        VersionedConstants::create_for_testing(),
        BouncerConfig::max(),
    )
}
