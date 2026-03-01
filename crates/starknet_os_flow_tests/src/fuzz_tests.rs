use std::collections::BTreeMap;
use std::sync::LazyLock;

use blockifier::test_utils::dict_state_reader::DictStateReader;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::calldata::create_calldata;
use blockifier_test_utils::contracts::FeatureContract;
use rand::prelude::IteratorRandom;
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha8Rng;
use rstest::rstest;
use starknet_api::core::{ClassHash, ContractAddress};
use starknet_api::state::StorageKey;
use starknet_api::{calldata, invoke_tx_args};
use starknet_committer::block_committer::input::StarknetStorageValue;
use starknet_types_core::felt::Felt;
use strum::{EnumIter, IntoEnumIterator};

use crate::test_manager::{TestBuilder, FUNDED_ACCOUNT_ADDRESS};
use crate::tests::NON_TRIVIAL_RESOURCE_BOUNDS;
use crate::utils::get_class_hash_of_feature_contract;

/// Contracts.
const ORCHESTRATOR_CONTRACT: FeatureContract =
    FeatureContract::FuzzTestOrchestrator(RunnableCairo1::Casm);
const CAIRO0_CONTRACT: FeatureContract = FeatureContract::FuzzTest(CairoVersion::Cairo0);
const CAIRO1_CONTRACT: FeatureContract =
    FeatureContract::FuzzTest(CairoVersion::Cairo1(RunnableCairo1::Casm));
const CAIRO1_REPLACEMENT_CONTRACT: FeatureContract =
    FeatureContract::FuzzTest2(RunnableCairo1::Casm);

/// Class hashes and class_hash -> is_cairo1 mapping.
static CAIRO0_CONTRACT_CLASS_HASH: LazyLock<ClassHash> =
    LazyLock::new(|| get_class_hash_of_feature_contract(CAIRO0_CONTRACT));
static CAIRO1_CONTRACT_CLASS_HASH: LazyLock<ClassHash> =
    LazyLock::new(|| get_class_hash_of_feature_contract(CAIRO1_CONTRACT));
static CAIRO1_REPLACEMENT_CLASS_HASH: LazyLock<ClassHash> =
    LazyLock::new(|| get_class_hash_of_feature_contract(CAIRO1_REPLACEMENT_CONTRACT));
#[allow(dead_code)]
static IS_CAIRO1: LazyLock<BTreeMap<ClassHash, bool>> = LazyLock::new(|| {
    BTreeMap::from([
        (*CAIRO0_CONTRACT_CLASS_HASH, false),
        (*CAIRO1_CONTRACT_CLASS_HASH, true),
        (*CAIRO1_REPLACEMENT_CLASS_HASH, true),
    ])
});

/// Storage key that can be written to.
static VALID_STORAGE_KEYS: LazyLock<Vec<Felt>> =
    LazyLock::new(|| vec![Felt::from(1u16 << 8), Felt::from(1u16 << 9)]);

#[derive(Clone, Copy, EnumIter)]
enum FuzzOperation {
    Return,
    Call,
    LibraryCall,
    Write,
    ReplaceClass,
}

impl FuzzOperation {
    fn identifier(&self) -> Felt {
        Felt::from(match self {
            Self::Return => 0u8,
            Self::Call => 1u8,
            Self::LibraryCall => 2u8,
            Self::Write => 3u8,
            Self::ReplaceClass => 4u8,
        })
    }
}

/// Different variants depending on whether or not the calling context is Cairo0.
#[derive(Clone, Copy, Debug)]
enum CallOperationData {
    Cairo0 { address: ContractAddress },
    Cairo1 { address: ContractAddress, unwraps_error: bool },
}

impl CallOperationData {
    pub fn felt_vector(&self) -> Vec<Felt> {
        match self {
            Self::Cairo0 { address } => vec![***address],
            Self::Cairo1 { address, unwraps_error } => {
                vec![***address, (*unwraps_error).into()]
            }
        }
    }

    pub fn address(&self) -> &ContractAddress {
        match self {
            Self::Cairo0 { address } | Self::Cairo1 { address, .. } => address,
        }
    }

    pub fn parent_failure_behavior(&self) -> ParentFailureBehavior {
        match self {
            Self::Cairo0 { .. } => ParentFailureBehavior::Uncatchable,
            Self::Cairo1 { unwraps_error, .. } => {
                ParentFailureBehavior::cairo1_behavior(*unwraps_error)
            }
        }
    }
}

/// Different variants depending on whether or not the calling context is Cairo0.
#[derive(Clone, Copy, Debug)]
enum LibraryCallOperationData {
    Cairo0 { class_hash: ClassHash },
    Cairo1 { class_hash: ClassHash, unwraps_error: bool },
}

impl LibraryCallOperationData {
    pub fn felt_vector(&self) -> Vec<Felt> {
        match self {
            Self::Cairo0 { class_hash, .. } => vec![class_hash.0],
            Self::Cairo1 { class_hash, unwraps_error, .. } => {
                vec![class_hash.0, (*unwraps_error).into()]
            }
        }
    }

    pub fn class_hash(&self) -> &ClassHash {
        match self {
            Self::Cairo0 { class_hash } | Self::Cairo1 { class_hash, .. } => class_hash,
        }
    }

    pub fn parent_failure_behavior(&self) -> ParentFailureBehavior {
        match self {
            Self::Cairo0 { .. } => ParentFailureBehavior::Uncatchable,
            Self::Cairo1 { unwraps_error, .. } => {
                ParentFailureBehavior::cairo1_behavior(*unwraps_error)
            }
        }
    }
}

/// Data associated with a fuzz operation.
#[derive(Clone, Copy)]
enum FuzzOperationData {
    Return,
    Call(CallOperationData),
    LibraryCall(LibraryCallOperationData),
    Write(StorageKey, StarknetStorageValue),
    ReplaceClass(ClassHash),
}

impl FuzzOperationData {
    pub fn op(&self) -> FuzzOperation {
        match self {
            Self::Return => FuzzOperation::Return,
            Self::Call(_) => FuzzOperation::Call,
            Self::LibraryCall(_) => FuzzOperation::LibraryCall,
            Self::Write(_, _) => FuzzOperation::Write,
            Self::ReplaceClass(_) => FuzzOperation::ReplaceClass,
        }
    }

    /// Convert the operation data to a vector of felt values that can be used as calldata for a
    /// fuzz test.
    pub fn felt_vector(&self) -> Vec<Felt> {
        let mut felt_vector = vec![self.op().identifier()];
        felt_vector.extend(match self {
            Self::Return => vec![],
            Self::Call(op) => op.felt_vector(),
            Self::LibraryCall(op) => op.felt_vector(),
            Self::Write(storage_key, value) => vec![***storage_key, value.0],
            Self::ReplaceClass(class_hash) => vec![class_hash.0],
        });
        felt_vector
    }
}

/// Parent frame behavior on failures.
enum ParentFailureBehavior {
    /// In a cairo0 context, or in a constructor call tree. Failures in this context cannot be
    /// caught by any calling context.
    Uncatchable,

    /// In cairo1, and not unwrapping errors from child context.
    Cairo1Catching,

    /// In cairo1, unwrapping errors from next context.
    Cairo1Propagating,
}

impl ParentFailureBehavior {
    pub fn cairo1_behavior(unwraps_error: bool) -> Self {
        if unwraps_error { Self::Cairo1Propagating } else { Self::Cairo1Catching }
    }
}

/// Final state of the fuzz test transaction.
enum FinalizedState {
    Ongoing,
    Succeeded,
}

impl FinalizedState {
    pub fn finalized(&self) -> bool {
        match self {
            Self::Ongoing => false,
            Self::Succeeded => true,
        }
    }
}

/// Similar to [CallInfo], but for a fuzz test. Represents the information of a single call in the
/// call tree.
#[allow(dead_code)]
struct FuzzCallInfo {
    pub address: ContractAddress,
    pub class_hash: ClassHash,
    pub parent_failure_behavior: ParentFailureBehavior,
    pub inner_calls: Vec<FuzzCallInfo>,
    /// If this is true, then the class was replaced in this frame.
    pub class_replaced_here: bool,
}

impl FuzzCallInfo {
    pub fn new_call(
        address: ContractAddress,
        class_hash: ClassHash,
        parent_failure_behavior: ParentFailureBehavior,
    ) -> Self {
        Self {
            address,
            class_hash,
            parent_failure_behavior,
            inner_calls: vec![],
            class_replaced_here: false,
        }
    }
}

/// Represents the call tree of a fuzz test.
#[allow(dead_code)]
struct FuzzTestManager {
    /// The call tree of the fuzz test.
    /// The first frame is the frame called by the orchestrator (it's parent frame is the
    /// orchestrator).
    pub calls: Vec<FuzzCallInfo>,

    /// The current call index in the call tree. For example if `current_call` is `[0, 1, 2]`, then
    /// the current call is `calls[0].inner_calls[1].inner_calls[2]`.
    pub current_call: Vec<usize>,

    /// The final state of the fuzz test.
    pub final_state: FinalizedState,

    /// List of operations applied to the fuzz test so far.
    pub operations: Vec<FuzzOperationData>,

    /// Deployed fuzz test contracts.
    pub deployed_fuzz_contracts: BTreeMap<ContractAddress, ClassHash>,

    /// We only allow one replace class per test.
    pub class_replaced: bool,

    /// Next value to write in a storage-write operation.
    pub next_storage_write_value: StarknetStorageValue,

    pub test_manager: TestBuilder<DictStateReader>,
    pub orchestrator_contract_address: ContractAddress,
    pub rng: ChaCha8Rng,
}

impl FuzzTestManager {
    pub async fn init(seed: u64) -> Self {
        // Initialize the state with:
        // - an orchestrator contract.
        // - two cairo1 fuzz test contracts.
        // - two cairo0 fuzz test contracts.
        let (
            mut test_manager,
            [
                orchestrator_contract_address,
                cairo1_contract_address_a,
                cairo1_contract_address_b,
                cairo0_contract_address_a,
                cairo0_contract_address_b,
                // We don't need an instance of the replacement class, but we do want it declared.
                _replacement_address,
            ],
        ) = TestBuilder::create_standard([
            (ORCHESTRATOR_CONTRACT, calldata![]),
            (CAIRO1_CONTRACT, calldata![Felt::ZERO]),
            (CAIRO1_CONTRACT, calldata![Felt::ZERO]),
            (CAIRO0_CONTRACT, calldata![Felt::ZERO]),
            (CAIRO0_CONTRACT, calldata![Felt::ZERO]),
            (CAIRO1_REPLACEMENT_CONTRACT, calldata![Felt::ZERO]),
        ])
        .await;

        let deployed_fuzz_contracts = BTreeMap::from([
            (cairo1_contract_address_a, *CAIRO1_CONTRACT_CLASS_HASH),
            (cairo1_contract_address_b, *CAIRO1_CONTRACT_CLASS_HASH),
            (cairo0_contract_address_a, *CAIRO0_CONTRACT_CLASS_HASH),
            (cairo0_contract_address_b, *CAIRO0_CONTRACT_CLASS_HASH),
        ]);

        // Initialize the fuzz testing contracts with the orchestrator address.
        for address in deployed_fuzz_contracts.keys() {
            let calldata =
                create_calldata(*address, "initialize", &[**orchestrator_contract_address]);
            test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });
        }

        // First call is the orchestrator calling the first fuzz test contract.
        let first_call = FuzzCallInfo::new_call(
            cairo1_contract_address_a,
            *CAIRO1_CONTRACT_CLASS_HASH,
            // The orchestrator always starts the test in a catching context.
            ParentFailureBehavior::Cairo1Catching,
        );
        Self {
            calls: vec![first_call],
            current_call: vec![0],
            final_state: FinalizedState::Ongoing,
            operations: vec![],
            deployed_fuzz_contracts,
            class_replaced: false,
            next_storage_write_value: StarknetStorageValue(Felt::from(1u16 << 12)),
            test_manager,
            orchestrator_contract_address,
            rng: ChaCha8Rng::seed_from_u64(seed),
        }
    }

    pub fn finalized(&self) -> bool {
        self.final_state.finalized()
    }

    pub fn current_fuzz_call_info(&self) -> &FuzzCallInfo {
        let mut call = &self.calls[self.current_call[0]];
        for i in 1..self.current_call.len() {
            call = &call.inner_calls[self.current_call[i]];
        }
        call
    }

    pub fn current_fuzz_call_info_mut(&mut self) -> &mut FuzzCallInfo {
        let mut call = &mut self.calls[self.current_call[0]];
        for i in 1..self.current_call.len() {
            call = &mut call.inner_calls[self.current_call[i]];
        }
        call
    }

    pub fn current_address(&self) -> ContractAddress {
        self.current_fuzz_call_info().address
    }

    pub fn current_class_hash(&self) -> ClassHash {
        self.current_fuzz_call_info().class_hash
    }

    pub fn is_cairo1_class(&self, class_hash: &ClassHash) -> bool {
        *IS_CAIRO1.get(class_hash).unwrap()
    }

    pub fn is_current_context_cairo1(&self) -> bool {
        self.is_cairo1_class(&self.current_class_hash())
    }

    /// Returns a vector of operations of the given type that can be applied on the current context.
    pub fn valid_operations_of_type(
        &self,
        operation_type: FuzzOperation,
    ) -> Vec<FuzzOperationData> {
        // No valid operations on finalized context.
        if self.finalized() {
            return vec![];
        }

        match operation_type {
            FuzzOperation::Return => vec![FuzzOperationData::Return],
            FuzzOperation::Call => {
                // There are two Cairo0 contracts and two Cairo1 contracts that can be called.
                // When calling from a Cairo1 context, the caller can unwrap the call result or not.
                let current_context_is_cairo1 = self.is_current_context_cairo1();
                self.deployed_fuzz_contracts
                    .keys()
                    .flat_map(|address| {
                        if current_context_is_cairo1 {
                            [true, false]
                                .into_iter()
                                .map(|unwraps_error| {
                                    FuzzOperationData::Call(CallOperationData::Cairo1 {
                                        address: *address,
                                        unwraps_error,
                                    })
                                })
                                .collect()
                        } else {
                            vec![FuzzOperationData::Call(CallOperationData::Cairo0 {
                                address: *address,
                            })]
                        }
                    })
                    .collect()
            }
            FuzzOperation::LibraryCall => {
                // We have one Cairo0 contract and two Cairo1 contracts to choose from.
                // Similar to calls, when calling from a Cairo1 context, the caller can unwrap the
                // call result or not.
                let current_context_is_cairo1 = self.is_current_context_cairo1();
                IS_CAIRO1
                    .keys()
                    .flat_map(|class_hash| {
                        if current_context_is_cairo1 {
                            [true, false]
                                .into_iter()
                                .map(|unwraps_error| {
                                    FuzzOperationData::LibraryCall(
                                        LibraryCallOperationData::Cairo1 {
                                            class_hash: *class_hash,
                                            unwraps_error,
                                        },
                                    )
                                })
                                .collect()
                        } else {
                            vec![FuzzOperationData::LibraryCall(LibraryCallOperationData::Cairo0 {
                                class_hash: *class_hash,
                            })]
                        }
                    })
                    .collect()
            }
            FuzzOperation::Write => VALID_STORAGE_KEYS
                .iter()
                .map(|storage_key| {
                    FuzzOperationData::Write(
                        StorageKey::try_from(*storage_key).unwrap(),
                        self.next_storage_write_value,
                    )
                })
                .collect(),
            FuzzOperation::ReplaceClass => {
                // If class was already replaced, no more replacements are allowed.
                if self.class_replaced {
                    return vec![];
                }
                vec![FuzzOperationData::ReplaceClass(*CAIRO1_REPLACEMENT_CLASS_HASH)]
            }
        }
    }

    /// List of all valid single operations that can be applied on the current context.
    pub fn valid_operations(&self) -> Vec<FuzzOperationData> {
        FuzzOperation::iter().flat_map(|op| self.valid_operations_of_type(op)).collect()
    }

    /// Applies the operation and updates the context.
    pub fn apply(&mut self, operation: FuzzOperationData) {
        assert!(!self.finalized());
        self.operations.push(operation);
        match operation {
            FuzzOperationData::Return => {
                self.current_call.pop();
                // If we returned to orchestrator context, no more operations can be applied.
                if self.current_call.is_empty() {
                    self.final_state = FinalizedState::Succeeded;
                }
            }
            FuzzOperationData::Call(call_operation_data) => {
                let address = *call_operation_data.address();
                let class_hash = *self.deployed_fuzz_contracts.get(&address).unwrap();
                self.current_fuzz_call_info_mut().inner_calls.push(FuzzCallInfo::new_call(
                    address,
                    class_hash,
                    call_operation_data.parent_failure_behavior(),
                ));
                self.current_call.push(self.current_fuzz_call_info().inner_calls.len() - 1);
            }
            FuzzOperationData::LibraryCall(library_call_operation_data) => {
                let current_address = self.current_address();
                self.current_fuzz_call_info_mut().inner_calls.push(FuzzCallInfo::new_call(
                    current_address,
                    *library_call_operation_data.class_hash(),
                    library_call_operation_data.parent_failure_behavior(),
                ));
                self.current_call.push(self.current_fuzz_call_info().inner_calls.len() - 1);
            }
            FuzzOperationData::Write(_, _) => {
                self.next_storage_write_value.0 += Felt::ONE;
            }
            FuzzOperationData::ReplaceClass(class_hash) => {
                assert!(!self.class_replaced);
                assert_eq!(class_hash, *CAIRO1_REPLACEMENT_CLASS_HASH);
                self.class_replaced = true;
                // Update the mapping from address to class hash, so subsequent calls to this
                // address will correctly use the new class hash.
                self.deployed_fuzz_contracts.insert(self.current_address(), class_hash);
                // Update the current call to mark that it was replaced at this point, to make it
                // easy to track if the change must be reverted mid-test.
                self.current_fuzz_call_info_mut().class_replaced_here = true;
                // Note: we do not mutate the class hash of this call, because in the current call
                // context the original class code is run. Only if this address is called again (via
                // call-contract) should the code change be reflected.
            }
        }
    }

    /// Add and apply a random operation.
    /// Returns an error if there are no valid operation to add.
    pub fn add_random_operation(&mut self) -> Result<(), ()> {
        let valid_operations = self.valid_operations();
        if valid_operations.is_empty() {
            return Err(());
        }
        let operation = *valid_operations.iter().choose(&mut self.rng).unwrap();
        self.apply(operation);
        Ok(())
    }

    /// Convert the list of operations to a vector of felt values that can be used as calldata for a
    /// fuzz test.
    pub fn operations_to_scenario_data(operations: &[FuzzOperationData]) -> Vec<Felt> {
        operations.iter().flat_map(|op| op.felt_vector()).collect()
    }

    /// Run the fuzz test. Should be called after the operations list is final (no need to finalize
    /// the context - if the finalized state is Ongoing it will be converted to Succeeded).
    pub async fn run_test(mut self) {
        if !self.finalized() {
            self.final_state = FinalizedState::Succeeded;
        }

        // Check the intended starting point by inspecting the first call.
        let first_called_address = self.calls[0].address;

        // Initialize the orchestrator contract with the scenario data.
        let scenario_data = Self::operations_to_scenario_data(&self.operations);
        let orchestrator_calldata = create_calldata(
            self.orchestrator_contract_address,
            "initialize",
            &[vec![Felt::from(scenario_data.len())], scenario_data].concat(),
        );
        self.test_manager
            .add_funded_account_invoke(invoke_tx_args! { calldata: orchestrator_calldata });

        // Invoke the test.
        let start_test_calldata = create_calldata(
            self.orchestrator_contract_address,
            "start_test",
            &[**first_called_address],
        );

        // Whether or not a revert is expected depends on context.
        let tx_revert_error = match self.final_state {
            FinalizedState::Succeeded => None,
            FinalizedState::Ongoing => unreachable!(),
        };
        let nonce = self.test_manager.next_nonce(*FUNDED_ACCOUNT_ADDRESS);
        self.test_manager.add_invoke_tx_from_args(
            invoke_tx_args! {
                sender_address: *FUNDED_ACCOUNT_ADDRESS,
                nonce,
                resource_bounds: *NON_TRIVIAL_RESOURCE_BOUNDS,
                calldata: start_test_calldata,
            },
            tx_revert_error,
        );

        // Run the test.
        let test_output = self.test_manager.build_and_run().await;
        test_output.perform_default_validations();
    }
}

#[rstest]
#[tokio::test]
async fn test_cairo1_revert_fuzz(
    #[values(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15)] seed: u64,
    #[values(1)] iterations: u64, // Easy way to multiply the number of test seeds.
) {
    for i in 0..iterations {
        let iteration_seed = seed + i * 16;
        let mut fuzz_tester = FuzzTestManager::init(iteration_seed).await;

        let max_n_operations = 10;

        // Create scenarios.
        for _ in 0..max_n_operations {
            // An error value means the context is finalized - no more operations can be applied.
            if fuzz_tester.add_random_operation().is_err() {
                break;
            }
        }

        fuzz_tester.run_test().await;
    }
}
