use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use blockifier::blockifier_versioned_constants::VersionedConstants;
use blockifier::bouncer::BouncerConfig;
use blockifier::context::{BlockContext, ChainInfo, FeeTokenAddresses};
use blockifier::state::cached_state::{CachedState, StateMaps};
use blockifier::state::state_api::{StateReader, UpdatableState};
use blockifier::state::stateful_compression_test_utils::decompress;
use blockifier::test_utils::dict_state_reader::DictStateReader;
use blockifier::test_utils::ALIAS_CONTRACT_ADDRESS;
use blockifier::transaction::objects::TransactionExecutionInfo;
use blockifier::transaction::transaction_execution::Transaction as BlockifierTransaction;
use blockifier_test_utils::calldata::create_calldata;
use blockifier_test_utils::contracts::FeatureContract;
use cairo_vm::types::builtin_name::BuiltinName;
use expect_test::{expect, Expect};
use itertools::Itertools;
use starknet_api::abi::abi_utils::{get_fee_token_var_address, selector_from_name};
use starknet_api::block::{BlockHash, BlockInfo, BlockNumber, PreviousBlockNumber};
use starknet_api::block_hash::block_hash_calculator::{
    calculate_block_hash,
    BlockHeaderCommitments,
    PartialBlockHashComponents,
};
use starknet_api::contract_class::compiled_class_hash::{HashVersion, HashableCompiledClass};
use starknet_api::contract_class::ContractClass;
use starknet_api::core::{
    ChainId,
    ClassHash,
    ContractAddress,
    EthAddress,
    GlobalRoot,
    Nonce,
    OsChainInfo,
    PatriciaKey,
};
use starknet_api::executable_transaction::{
    AccountTransaction,
    DeclareTransaction,
    DeployAccountTransaction,
    InvokeTransaction,
    L1HandlerTransaction as ExecutableL1HandlerTransaction,
    Transaction as ExecutableTransaction,
};
use starknet_api::hash::StateRoots;
use starknet_api::invoke_tx_args;
use starknet_api::state::{SierraContractClass, StorageKey};
use starknet_api::test_utils::invoke::{invoke_tx, InvokeTxArgs};
use starknet_api::test_utils::{NonceManager, CHAIN_ID_FOR_TESTS};
use starknet_api::transaction::fields::{Calldata, Fee, Tip};
use starknet_api::transaction::{L1HandlerTransaction, L1ToL2Payload, MessageToL1};
use starknet_committer::block_committer::input::{
    IsSubset,
    StarknetStorageKey,
    StarknetStorageValue,
    StateDiff,
};
use starknet_committer::db::facts_db::FactsDb;
use starknet_committer::db::forest_trait::StorageInitializer;
use starknet_os::commitment_infos::create_commitment_infos;
use starknet_os::hints::hint_implementation::state_diff_encryption::utils::compute_public_keys;
use starknet_os::io::os_input::{OsBlockInput, OsHints, OsHintsConfig, StarknetOsInput};
use starknet_os::io::os_output::{MessageToL2, OsStateDiff, StarknetOsRunnerOutput};
use starknet_os::io::os_output_types::{
    FullOsStateDiff,
    PartialCommitmentOsStateDiff,
    PartialOsStateDiff,
    TryFromOutputIter,
};
use starknet_os::io::test_utils::validate_kzg_segment;
use starknet_os::runner::{run_os_stateless, DEFAULT_OS_LAYOUT};
use starknet_os::test_utils::coverage::expect_hint_coverage;
use starknet_os_runner::committer_utils::{commit_state_diff, state_maps_to_committer_state_diff};
use starknet_types_core::felt::Felt;

use crate::initial_state::{
    create_default_initial_state_data,
    get_initial_deploy_account_tx,
    FlowTestState,
    InitialState,
    InitialStateData,
    OsExecutionContracts,
};
use crate::tests::NON_TRIVIAL_RESOURCE_BOUNDS;
use crate::utils::{
    divide_vec_into_n_parts,
    execute_transactions,
    get_extended_initial_reads,
    maybe_dummy_block_hash_and_number,
    ExecutionOutput,
};

/// The STRK fee token address that was deployed when initializing the default initial state.
/// The resulting address depends on the nonce of the deploying account - if extra init transactions
/// are added to the initial state construction before the STRK fee token is deployed, the address
/// must be updated.
pub(crate) const EXPECTED_STRK_FEE_TOKEN_ADDRESS: Expect = expect![
    r#"
    0x4a058b5cfd03175ed4bf39ef9613319c8ffaa0380e0ec4c27b5ab76c642ed54
"#
];
pub(crate) static STRK_FEE_TOKEN_ADDRESS: LazyLock<ContractAddress> = LazyLock::new(|| {
    ContractAddress(
        PatriciaKey::try_from(Felt::from_hex_unchecked(
            EXPECTED_STRK_FEE_TOKEN_ADDRESS.data.trim(),
        ))
        .unwrap(),
    )
});
/// The address of a funded account that is able to pay fees for transactions.
/// This address was initialized when creating the default initial state.
pub(crate) static FUNDED_ACCOUNT_ADDRESS: LazyLock<ContractAddress> =
    LazyLock::new(|| get_initial_deploy_account_tx().contract_address);

#[derive(Default)]
pub(crate) struct TestBuilderConfig {
    pub(crate) use_kzg_da: bool,
    pub(crate) full_output: bool,
    pub(crate) private_keys: Option<Vec<Felt>>,
}

pub(crate) struct FlowTestTx {
    tx: BlockifierTransaction,
    expected_revert_reason: Option<String>,
}

pub(crate) struct OsTestExpectedValues {
    pub(crate) previous_global_root: GlobalRoot,
    pub(crate) new_global_root: GlobalRoot,
    pub(crate) previous_block_number: PreviousBlockNumber,
    pub(crate) new_block_number: BlockNumber,
    pub(crate) previous_block_hash: BlockHash,
    pub(crate) new_block_hash: BlockHash,
    pub(crate) config_hash: Felt,
    pub(crate) use_kzg_da: bool,
    pub(crate) full_output: bool,
    pub(crate) messages_to_l1: Vec<MessageToL1>,
    pub(crate) messages_to_l2: Vec<MessageToL2>,
    pub(crate) committed_state_diff: StateDiff,
}

impl OsTestExpectedValues {
    pub(crate) fn new(
        os_hints: &OsHints,
        messages_to_l1: Vec<MessageToL1>,
        messages_to_l2: Vec<MessageToL2>,
        committed_state_diff: StateDiff,
    ) -> Self {
        let first_block = os_hints.os_input.os_block_inputs.first().unwrap();
        let last_block = os_hints.os_input.os_block_inputs.last().unwrap();

        // Compute global roots from commitment infos.
        let previous_global_root = StateRoots {
            contracts_trie_root_hash: first_block.contract_state_commitment_info.previous_root,
            classes_trie_root_hash: first_block.contract_class_commitment_info.previous_root,
        }
        .global_root();
        let new_global_root = StateRoots {
            contracts_trie_root_hash: last_block.contract_state_commitment_info.updated_root,
            classes_trie_root_hash: last_block.contract_class_commitment_info.updated_root,
        }
        .global_root();

        // Config hash and flags.
        let config = &os_hints.os_hints_config;
        let config_hash =
            config.chain_info.compute_os_config_hash(config.public_keys.as_ref()).unwrap();
        Self {
            previous_global_root,
            new_global_root,
            previous_block_number: PreviousBlockNumber(first_block.block_info.block_number.prev()),
            new_block_number: last_block.block_info.block_number,
            previous_block_hash: first_block.prev_block_hash,
            new_block_hash: last_block.new_block_hash,
            config_hash,
            // The OS will not compute a KZG commitment in full output mode.
            use_kzg_da: config.use_kzg_da && !config.full_output,
            full_output: config.full_output,
            messages_to_l1,
            messages_to_l2,
            committed_state_diff,
        }
    }
}

pub(crate) struct OsTestOutput<S: StateReader> {
    pub(crate) runner_output: StarknetOsRunnerOutput,
    pub(crate) private_keys: Option<Vec<Felt>>,
    pub(crate) decompressed_state_diff: StateDiff,
    pub(crate) final_state: S,
    pub(crate) expected_values: OsTestExpectedValues,
}

impl<S: FlowTestState> OsTestOutput<S> {
    pub(crate) fn get_builtin_usage(&self, builtin_name: &BuiltinName) -> usize {
        *self
            .runner_output
            .metrics
            .execution_resources
            .builtin_instance_counter
            .get(builtin_name)
            .unwrap()
    }

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

    #[track_caller]
    pub(crate) fn assert_storage_diff_eq(
        &self,
        contract_address: ContractAddress,
        storage_updates: HashMap<Felt, Felt>,
    ) {
        assert_eq!(
            self.decompressed_state_diff
                .storage_updates
                .get(&contract_address)
                .unwrap_or(&HashMap::default()),
            &storage_updates
                .into_iter()
                .map(|(key, value)| (
                    StarknetStorageKey(key.try_into().unwrap()),
                    StarknetStorageValue(value)
                ))
                .collect::<HashMap<_, _>>()
        );
    }

    fn perform_global_validations(&self) {
        // TODO(Dori): Implement global validations for the OS test output.

        // Builtin validations are done in `run_os_stateless_for_testing`.

        let os_output = self
            .runner_output
            .get_os_output(self.private_keys.as_ref())
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

        // Block hashes.
        assert_eq!(
            os_output.common_os_output.prev_block_hash,
            self.expected_values.previous_block_hash.0
        );
        assert_eq!(
            os_output.common_os_output.new_block_hash,
            self.expected_values.new_block_hash.0
        );

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

    pub(crate) fn expect_hint_coverage(&self, test_name: &str) {
        expect_hint_coverage(&self.runner_output.unused_hints, test_name);
    }
}

/// Holds the data needed to run the OS and create the test output.
pub(crate) struct TestRunner<S: FlowTestState> {
    pub(crate) os_hints: OsHints,
    // Carries the writes of the entire execution in its cache.
    pub(crate) entire_cached_state: CachedState<S>,
    pub(crate) messages_to_l1: Vec<MessageToL1>,
    pub(crate) messages_to_l2: Vec<MessageToL2>,
    pub(crate) private_keys: Option<Vec<Felt>>,
}

impl<S: FlowTestState> TestRunner<S> {
    /// Runs the OS and creates the test output.
    pub(crate) fn run(mut self) -> OsTestOutput<S> {
        // This cached state holds the diff of the entire execution.
        let entire_state_diff = self.entire_cached_state.to_state_diff().unwrap().state_maps;
        let entire_initial_reads = get_extended_initial_reads(&self.entire_cached_state);
        self.entire_cached_state.state.apply_writes(
            &entire_state_diff,
            &self.entire_cached_state.class_hash_to_class.borrow(),
        );
        let final_state = self.entire_cached_state.state;

        // Create expected values before running OS (os_hints is consumed by run_os_stateless).
        let expected_values = OsTestExpectedValues::new(
            &self.os_hints,
            self.messages_to_l1,
            self.messages_to_l2,
            state_maps_to_committer_state_diff(entire_state_diff.clone()),
        );
        let layout = DEFAULT_OS_LAYOUT;
        let os_output = run_os_stateless(layout, self.os_hints).unwrap();

        let decompressed_state_diff =
            state_maps_to_committer_state_diff(TestBuilder::<S>::get_decompressed_state_diff(
                &os_output,
                &final_state,
                entire_initial_reads.alias_keys(),
                self.private_keys.as_ref(),
            ));

        OsTestOutput {
            runner_output: os_output,
            private_keys: self.private_keys,
            decompressed_state_diff,
            final_state,
            expected_values,
        }
    }
}

/// Builds flow tests by maintaining the initial state and transactions.
/// Use the builder methods to configure transactions, then call `build()` to get a `TestRunner`.
pub(crate) struct TestBuilder<S: FlowTestState> {
    pub(crate) initial_state: InitialState<S>,
    pub(crate) nonce_manager: NonceManager,
    pub(crate) execution_contracts: OsExecutionContracts,
    pub(crate) os_hints_config: OsHintsConfig,
    pub(crate) private_keys: Option<Vec<Felt>>,
    pub(crate) virtual_os: bool,
    pub(crate) messages_to_l1: Vec<MessageToL1>,
    pub(crate) messages_to_l2: Vec<MessageToL2>,

    per_block_txs: Vec<Vec<FlowTestTx>>,
}

impl<S: FlowTestState> TestBuilder<S> {
    /// Creates a new `TestBuilder` with the provided initial state data.
    pub(crate) fn new_with_initial_state_data(
        initial_state_data: InitialStateData<S>,
        config: TestBuilderConfig,
        virtual_os: bool,
    ) -> Self {
        let public_keys =
            config.private_keys.as_ref().map(|private_keys| compute_public_keys(private_keys));
        let chain_info =
            OsChainInfo::from(initial_state_data.initial_state.block_context.chain_info());
        let os_hints_config = OsHintsConfig {
            chain_info,
            use_kzg_da: config.use_kzg_da,
            full_output: config.full_output,
            public_keys,
            debug_mode: false,
            rng_seed_salt: None,
        };
        Self {
            initial_state: initial_state_data.initial_state,
            nonce_manager: initial_state_data.nonce_manager,
            execution_contracts: initial_state_data.execution_contracts,
            os_hints_config,
            private_keys: config.private_keys,
            virtual_os,
            messages_to_l1: Vec::new(),
            messages_to_l2: Vec::new(),
            per_block_txs: vec![vec![]],
        }
    }

    /// Creates a new `TestBuilder` with the default initial state.
    /// Optionally provide an array of extra contracts to declare and deploy - the addresses of
    /// these contracts will be returned as an array of the same length.
    pub(crate) async fn new_with_default_initial_state<const N: usize>(
        extra_contracts: [(FeatureContract, Calldata); N],
        config: TestBuilderConfig,
        virtual_os: bool,
    ) -> (Self, [ContractAddress; N]) {
        let (default_initial_state_data, extra_addresses) =
            create_default_initial_state_data::<S, N>(extra_contracts).await;
        (
            Self::new_with_initial_state_data(default_initial_state_data, config, virtual_os),
            extra_addresses,
        )
    }

    pub(crate) fn next_nonce(&mut self, account_address: ContractAddress) -> Nonce {
        self.nonce_manager.next(account_address)
    }

    pub(crate) fn get_nonce(&self, account_address: ContractAddress) -> Nonce {
        self.nonce_manager.get(account_address)
    }

    /// Returns the first block number to be used for execution (based on the initial state).
    pub(crate) fn first_block_number(&self) -> BlockNumber {
        self.initial_state.block_context.block_info().block_number.next().unwrap()
    }

    /// Returns the base (previous) block info from the initial state.
    /// In virtual OS mode, this is the block info returned by get_execution_info.
    pub(crate) fn base_block_info(&self) -> &BlockInfo {
        self.initial_state.block_context.block_info()
    }

    /// Returns the chain ID from the initial state's block context.
    pub(crate) fn chain_id(&self) -> ChainId {
        self.initial_state.block_context.chain_info().chain_id.clone()
    }

    /// Computes the virtual OS config hash for proof facts validation using the test environment's
    /// chain info.
    pub(crate) fn compute_virtual_os_config_hash(&self) -> Felt {
        let chain_info = self.initial_state.block_context.chain_info();
        OsChainInfo::from(chain_info).compute_virtual_os_config_hash().unwrap()
    }

    /// Advances the manager to the next block when adding new transactions.
    pub(crate) fn move_to_next_block(&mut self) {
        // TODO(Yoni): add here useful block info fields like timestamp.
        self.per_block_txs.push(vec![]);
    }

    pub(crate) fn total_txs(&self) -> usize {
        self.per_block_txs.iter().map(|block| block.len()).sum()
    }

    fn last_block_txs_mut(&mut self) -> &mut Vec<FlowTestTx> {
        self.per_block_txs
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
            tx: BlockifierTransaction::new_for_sequencing(ExecutableTransaction::Account(
                AccountTransaction::Declare(tx),
            )),
            expected_revert_reason: None,
        });

        self.execution_contracts
            .declared_class_hash_to_component_hashes
            .insert(sierra.calculate_class_hash(), sierra.get_component_hashes());
        let compiled_class_hash = casm.hash(&HashVersion::V2);
        self.execution_contracts.executed.contracts.insert(compiled_class_hash, casm.clone());
    }

    pub(crate) fn add_invoke_tx(
        &mut self,
        tx: InvokeTransaction,
        expected_revert_reason: Option<String>,
    ) {
        self.last_block_txs_mut().push(FlowTestTx {
            tx: BlockifierTransaction::new_for_sequencing(ExecutableTransaction::Account(
                AccountTransaction::Invoke(tx),
            )),
            expected_revert_reason,
        });
    }

    pub(crate) fn add_invoke_tx_from_args(
        &mut self,
        args: InvokeTxArgs,
        revert_reason: Option<String>,
    ) {
        self.add_invoke_tx(
            InvokeTransaction::create(invoke_tx(args), &self.chain_id()).unwrap(),
            revert_reason,
        );
    }

    /// Similar to `add_invoke_tx_from_args`, but with the sender address set to the funded account,
    /// nonce set (and incremented) and resource bounds set to the default (non-trivial).
    /// Assumes the tx should not be reverted.
    pub(crate) fn add_funded_account_invoke(&mut self, additional_args: InvokeTxArgs) {
        let tx = self.create_funded_account_invoke(additional_args);
        self.add_invoke_tx(tx, None);
    }

    /// Creates an invoke transaction from the funded account, with nonce set (and incremented)
    /// and resource bounds set to the default (non-trivial).
    pub(crate) fn create_funded_account_invoke(
        &mut self,
        additional_args: InvokeTxArgs,
    ) -> InvokeTransaction {
        let nonce = self.next_nonce(*FUNDED_ACCOUNT_ADDRESS);
        InvokeTransaction::create(
            invoke_tx(InvokeTxArgs {
                sender_address: *FUNDED_ACCOUNT_ADDRESS,
                nonce,
                resource_bounds: *NON_TRIVIAL_RESOURCE_BOUNDS,
                ..additional_args
            }),
            &self.chain_id(),
        )
        .unwrap()
    }

    pub(crate) fn add_cairo0_declare_tx(&mut self, tx: DeclareTransaction, class_hash: ClassHash) {
        let ContractClass::V0(class) = tx.class_info.contract_class.clone() else {
            panic!("Expected a V0 contract class");
        };
        self.last_block_txs_mut().push(FlowTestTx {
            tx: BlockifierTransaction::new_for_sequencing(ExecutableTransaction::Account(
                AccountTransaction::Declare(tx),
            )),
            expected_revert_reason: None,
        });
        self.execution_contracts.executed.deprecated_contracts.insert(class_hash, class);
    }

    pub(crate) fn add_deploy_account_tx(&mut self, tx: DeployAccountTransaction) {
        self.last_block_txs_mut().push(FlowTestTx {
            tx: BlockifierTransaction::new_for_sequencing(ExecutableTransaction::Account(
                AccountTransaction::DeployAccount(tx),
            )),
            expected_revert_reason: None,
        });
    }

    pub(crate) fn add_l1_handler_tx(
        &mut self,
        tx: ExecutableL1HandlerTransaction,
        expected_revert_reason: Option<String>,
    ) {
        // If the transaction is not expected to revert, add the corresponding message-to-L2.
        if expected_revert_reason.is_none() {
            let calldata = &tx.tx.calldata.0;
            self.messages_to_l2.push(MessageToL2 {
                from_address: EthAddress::try_from(calldata[0]).unwrap(),
                to_address: tx.tx.contract_address,
                nonce: tx.tx.nonce,
                selector: tx.tx.entry_point_selector,
                payload: L1ToL2Payload(calldata[1..].to_vec()),
            });
        }
        self.last_block_txs_mut().push(FlowTestTx {
            tx: BlockifierTransaction::new_for_sequencing(ExecutableTransaction::L1Handler(tx)),
            expected_revert_reason,
        });
    }

    pub(crate) fn add_l1_handler(
        &mut self,
        contract_address: ContractAddress,
        entry_point_name: &str,
        calldata: Calldata,
        expected_revert_reason: Option<String>,
    ) {
        let tx = ExecutableL1HandlerTransaction::create(
            L1HandlerTransaction {
                version: L1HandlerTransaction::VERSION,
                nonce: Nonce::default(),
                contract_address,
                entry_point_selector: selector_from_name(entry_point_name),
                calldata,
            },
            &self.chain_id(),
            Fee(1_000_000),
        )
        .unwrap();
        self.add_l1_handler_tx(tx, expected_revert_reason);
    }

    pub(crate) fn add_fund_address_tx_with_default_amount(&mut self, address: ContractAddress) {
        let transfer_amount = 2 * NON_TRIVIAL_RESOURCE_BOUNDS.max_possible_fee(Tip(0)).0;
        self.add_fund_address_tx(address, transfer_amount);
    }

    pub(crate) fn add_fund_address_tx(&mut self, address: ContractAddress, amount: u128) {
        let calldata = create_calldata(
            *STRK_FEE_TOKEN_ADDRESS,
            "transfer",
            &[**address, Felt::from(amount), Felt::ZERO],
        );
        self.add_funded_account_invoke(invoke_tx_args! { calldata });
    }

    /// Divides the current transactions into the specified number of blocks.
    /// Panics if there is not exactly one block to divide.
    pub(crate) fn divide_transactions_into_n_blocks(&mut self, n_blocks: usize) {
        assert_eq!(
            self.per_block_txs.len(),
            1,
            "There should be only one block of transactions to divide."
        );
        self.per_block_txs = divide_vec_into_n_parts(self.per_block_txs.pop().unwrap(), n_blocks);
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
        private_keys: Option<&Vec<Felt>>,
    ) -> StateMaps {
        let os_output = runner_output
            .get_os_output(private_keys)
            .expect("Getting OsOutput from raw OS output should not fail.");
        let os_state_diff_maps = match os_output.state_diff {
            OsStateDiff::Partial(ref partial_os_state_diff) => {
                partial_os_state_diff.as_state_maps()
            }
            OsStateDiff::Full(ref full_os_state_diff) => full_os_state_diff.as_state_maps(),
            // In commitment modes, state diff should be deserialized from the DA segment.
            OsStateDiff::PartialCommitment(_) => {
                let da_segment = runner_output.da_segment.clone().unwrap();
                PartialOsStateDiff::try_from_output_iter(&mut da_segment.into_iter(), private_keys)
                    .unwrap()
                    .as_state_maps()
            }
            OsStateDiff::FullCommitment(_) => {
                let da_segment = runner_output.da_segment.clone().unwrap();
                FullOsStateDiff::try_from_output_iter(&mut da_segment.into_iter(), private_keys)
                    .unwrap()
                    .as_state_maps()
            }
        };
        decompress(&os_state_diff_maps, state, *ALIAS_CONTRACT_ADDRESS, alias_keys)
    }

    /// Builds the test runner from the current state and transactions.
    /// Returns a `TestRunner` that can be used to run the OS and get the test output.
    pub(crate) async fn build(self) -> TestRunner<S> {
        // TODO(Yoni): make this func sync.
        let mut os_block_inputs = vec![];
        let mut state = CachedState::new(self.initial_state.updatable_state);
        let mut map_storage = self.initial_state.commitment_storage;
        let base_block_context = self.initial_state.block_context;

        // The state roots updated after each block.
        let base_block_state_roots = StateRoots {
            contracts_trie_root_hash: self.initial_state.contracts_trie_root_hash,
            classes_trie_root_hash: self.initial_state.classes_trie_root_hash,
        };
        let mut previous_state_roots = base_block_state_roots;

        let use_kzg_da = self.os_hints_config.use_kzg_da;
        let mut current_block_hash = BlockHash::default();
        for (block_index, block_txs_with_reason) in self.per_block_txs.into_iter().enumerate() {
            let block_context = if self.virtual_os {
                // In virtual OS mode, the block context is the same as the base block context.
                base_block_context.clone()
            } else {
                BlockContext::from_base_context(&base_block_context, block_index, use_kzg_da)
            };

            let (block_txs, revert_reasons): (Vec<_>, Vec<_>) = block_txs_with_reason
                .into_iter()
                .map(|flow_test_tx| (flow_test_tx.tx, flow_test_tx.expected_revert_reason))
                .unzip();
            // Clone the block info for later use.
            let block_info = block_context.block_info().clone();
            // Execute the transactions.
            let ExecutionOutput { execution_outputs, mut final_state } =
                execute_transactions::<CachedState<S>>(
                    state,
                    &block_txs,
                    block_context,
                    self.virtual_os,
                );
            Self::verify_execution_outputs(block_index, &revert_reasons, &execution_outputs);
            let initial_reads = get_extended_initial_reads(&final_state);
            // Update the wrapped state.
            let state_diff = final_state.to_state_diff().unwrap().state_maps;
            state = final_state.state;
            state.apply_writes(&state_diff, &final_state.class_hash_to_class.borrow());
            // Commit the state diff.
            let committer_state_diff = state_maps_to_committer_state_diff(state_diff);
            let mut db = FactsDb::new(map_storage);
            let new_state_roots = commit_state_diff(
                &mut db,
                previous_state_roots.contracts_trie_root_hash,
                previous_state_roots.classes_trie_root_hash,
                committer_state_diff,
            )
            .await
            .expect("Failed to commit state diff.");
            map_storage = db.consume_storage();

            // Prepare the OS input.
            let commitment_infos = create_commitment_infos(
                &previous_state_roots,
                &new_state_roots,
                &mut map_storage,
                &initial_reads.keys(),
            )
            .await
            .unwrap();
            let tx_execution_infos = execution_outputs
                .into_iter()
                .map(|(execution_info, _)| execution_info.into())
                .collect();

            // TODO(Nimrod): Remove dummy block hashes once the OS verifies them.
            let old_block_number_and_hash =
                maybe_dummy_block_hash_and_number(block_info.block_number);
            let prev_block_hash = current_block_hash;
            let block_hash_commitments = BlockHeaderCommitments::default();
            let block_hash_state_root = if self.virtual_os {
                base_block_state_roots.global_root()
            } else {
                new_state_roots.global_root()
            };
            let new_block_hash = calculate_block_hash(
                &PartialBlockHashComponents::new(&block_info, block_hash_commitments.clone()),
                block_hash_state_root,
                prev_block_hash,
            )
            .unwrap();
            current_block_hash = new_block_hash;

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
                block_hash_commitments,
                old_block_number_and_hash,
                class_hashes_to_migrate: Vec::new(),
                initial_reads,
            };
            os_block_inputs.push(os_block_input);
            previous_state_roots = new_state_roots;
        }
        let starknet_os_input = StarknetOsInput {
            os_block_inputs,
            deprecated_compiled_classes: self.execution_contracts.executed.deprecated_contracts,
            compiled_classes: self.execution_contracts.executed.contracts,
        };
        let os_hints =
            OsHints { os_input: starknet_os_input, os_hints_config: self.os_hints_config };

        TestRunner {
            os_hints,
            entire_cached_state: state,
            messages_to_l1: self.messages_to_l1,
            messages_to_l2: self.messages_to_l2,
            private_keys: self.private_keys,
        }
    }

    /// Builds and runs the test, returning the test output.
    pub(crate) async fn build_and_run(self) -> OsTestOutput<S> {
        self.build().await.run()
    }
}

impl TestBuilder<DictStateReader> {
    pub(crate) async fn create_standard<const N: usize>(
        extra_contracts: [(FeatureContract, Calldata); N],
    ) -> (Self, [ContractAddress; N]) {
        Self::create_standard_with_config(extra_contracts, TestBuilderConfig::default()).await
    }

    /// Creates a new `TestBuilder` with the default initial state and the provided config.
    /// Uses `DictStateReader` as the state type.
    /// Returns the manager and an array of addresses for any extra contracts deployed.
    pub(crate) async fn create_standard_with_config<const N: usize>(
        extra_contracts: [(FeatureContract, Calldata); N],
        config: TestBuilderConfig,
    ) -> (Self, [ContractAddress; N]) {
        Self::new_with_default_initial_state(extra_contracts, config, false).await
    }

    pub(crate) async fn create_standard_virtual<const N: usize>(
        extra_contracts: [(FeatureContract, Calldata); N],
    ) -> (Self, [ContractAddress; N]) {
        Self::new_with_default_initial_state(extra_contracts, TestBuilderConfig::default(), true)
            .await
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
