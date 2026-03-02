use std::collections::{BTreeMap, BTreeSet};

use blockifier::test_utils::dict_state_reader::DictStateReader;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::calldata::create_calldata;
use blockifier_test_utils::contracts::FeatureContract;
use chrono::{Datelike, Utc};
use rand::prelude::IteratorRandom;
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha8Rng;
use rstest::rstest;
use starknet_api::core::{calculate_contract_address, ClassHash, ContractAddress};
use starknet_api::state::StorageKey;
use starknet_api::transaction::fields::ContractAddressSalt;
use starknet_api::{calldata, invoke_tx_args};
use starknet_committer::block_committer::input::StarknetStorageValue;
use starknet_types_core::felt::Felt;
use strum::{EnumIter, IntoEnumIterator};

use crate::test_manager::{TestBuilder, FUNDED_ACCOUNT_ADDRESS};
use crate::tests::NON_TRIVIAL_RESOURCE_BOUNDS;
use crate::utils::get_class_hash_of_feature_contract;

#[derive(Clone, Copy, EnumIter)]
enum FuzzOperation {
    Return,
    Call,
    LibraryCall,
    Write,
    ReplaceClass,
    Deploy,
    Panic,
}

impl FuzzOperation {
    fn identifier(&self) -> Felt {
        Felt::from(match self {
            Self::Return => 0u8,
            Self::Call => 1u8,
            Self::LibraryCall => 2u8,
            Self::Write => 3u8,
            Self::ReplaceClass => 4u8,
            Self::Deploy => 5u8,
            Self::Panic => 6u8,
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
    pub fn to_felt_vector(&self) -> Vec<Felt> {
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
    pub fn to_felt_vector(&self) -> Vec<Felt> {
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
#[derive(Clone, Copy, Debug)]
enum FuzzOperationData {
    Return,
    Call(CallOperationData),
    LibraryCall(LibraryCallOperationData),
    Write(StorageKey, StarknetStorageValue),
    ReplaceClass(ClassHash),
    Deploy { class_hash: ClassHash, salt: ContractAddressSalt },
    Panic,
}

impl FuzzOperationData {
    pub fn op(&self) -> FuzzOperation {
        match self {
            Self::Return => FuzzOperation::Return,
            Self::Call(_) => FuzzOperation::Call,
            Self::LibraryCall(_) => FuzzOperation::LibraryCall,
            Self::Write(_, _) => FuzzOperation::Write,
            Self::ReplaceClass(_) => FuzzOperation::ReplaceClass,
            Self::Deploy { .. } => FuzzOperation::Deploy,
            Self::Panic => FuzzOperation::Panic,
        }
    }

    /// Convert the operation data to a vector of felt values that can be used as calldata for a
    /// fuzz test.
    pub fn to_felt_vector(&self) -> Vec<Felt> {
        let mut felt_vector = vec![Felt::from(self.op().identifier())];
        felt_vector.extend(match self {
            Self::Return | Self::Panic => vec![],
            Self::Call(op) => op.to_felt_vector(),
            Self::LibraryCall(op) => op.to_felt_vector(),
            Self::Write(key, value) => vec![***key, value.0],
            Self::ReplaceClass(class_hash) => vec![class_hash.0],
            Self::Deploy { class_hash, salt } => vec![class_hash.0, salt.0],
        });
        felt_vector
    }
}

/// Parent frame behavior on failures.
#[derive(Debug, PartialEq)]
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
    Reverted,
    Succeeded,
}

impl FinalizedState {
    pub fn finalized(&self) -> bool {
        match self {
            Self::Ongoing => false,
            Self::Reverted | Self::Succeeded => true,
        }
    }
}

/// Similar to [CallInfo], but for a fuzz test. Represents the information of a single call in the
/// call tree.
struct FuzzCallInfo {
    pub address: ContractAddress,
    pub class_hash: ClassHash,
    pub parent_failure_behavior: ParentFailureBehavior,
    pub inner_calls: Vec<FuzzCallInfo>,
    /// If this is true, then the class was replaced in this frame.
    pub class_replaced_here: bool,
    /// If true, this call is a constructor; the address of this call is not a valid call address
    /// until this call returns.
    pub constructing: bool,
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
            constructing: false,
        }
    }

    pub fn new_deploy(address: ContractAddress, class_hash: ClassHash) -> Self {
        Self {
            address,
            class_hash,
            // Failures in constructors cannot be caught.
            parent_failure_behavior: ParentFailureBehavior::Uncatchable,
            inner_calls: vec![],
            class_replaced_here: false,
            constructing: true,
        }
    }
}

/// Data associated with a revert operation.
struct RevertInfo {
    /// Addresses of deployed contracts that were deployed in the reverted call tree.
    pub deployed_addresses: BTreeSet<ContractAddress>,

    /// If a class was replaced in the reverted call tree, the address and original class hash of
    /// the replaced class.
    pub class_replaced_and_original_class_hash: Option<(ContractAddress, ClassHash)>,
}

impl RevertInfo {
    pub fn combine(others: Vec<Self>) -> Self {
        // If there is a replace-class, there should be only one.
        let mut class_replaced_and_original_class_hash = None;
        for other in others.iter() {
            if let Some(inner_replacement) = other.class_replaced_and_original_class_hash {
                class_replaced_and_original_class_hash = Some(inner_replacement);
            }
        }
        Self {
            deployed_addresses: others
                .into_iter()
                .flat_map(|other| other.deployed_addresses)
                .collect(),
            class_replaced_and_original_class_hash,
        }
    }
}

/// Represents the call tree of a fuzz test.
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

    /// Map from contract address to class hash.
    pub deployed_contracts: BTreeMap<ContractAddress, ClassHash>,

    /// Undeployed class hash, for replacement or for library calls.
    pub cairo1_replacement_class_hash: ClassHash,

    /// We only allow one replace class per test.
    pub class_replaced: bool,

    /// Which classes are Cairo1. This data is static, and added as a field for convenience.
    pub is_cairo1: BTreeMap<ClassHash, bool>,

    /// Storage key that can be written to.
    pub valid_storage_keys: Vec<Felt>,

    /// Next value to write in a storage-write operation.
    pub next_storage_write_value: StarknetStorageValue,

    /// Next salt to use for a deploy operation.
    pub next_salt: ContractAddressSalt,

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
        let orchestrator_contract = FeatureContract::FuzzTestOrchestrator(RunnableCairo1::Casm);
        let cairo1_contract = FeatureContract::FuzzTest(CairoVersion::Cairo1(RunnableCairo1::Casm));
        let cairo0_contract = FeatureContract::FuzzTest(CairoVersion::Cairo0);
        let cairo1_replacement_contract = FeatureContract::FuzzTest2(RunnableCairo1::Casm);
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
            (orchestrator_contract, calldata![]),
            (cairo1_contract, calldata![Felt::ZERO, Felt::ZERO]),
            (cairo1_contract, calldata![Felt::ZERO, Felt::ZERO]),
            (cairo0_contract, calldata![Felt::ZERO, Felt::ZERO]),
            (cairo0_contract, calldata![Felt::ZERO, Felt::ZERO]),
            (cairo1_replacement_contract, calldata![Felt::ZERO, Felt::ZERO]),
        ])
        .await;

        let cairo1_replacement_class_hash =
            get_class_hash_of_feature_contract(cairo1_replacement_contract);
        let cairo1_contract_class_hash = get_class_hash_of_feature_contract(cairo1_contract);
        let cairo0_contract_class_hash = get_class_hash_of_feature_contract(cairo0_contract);

        let deployed_fuzz_contracts = BTreeMap::from([
            (cairo1_contract_address_a, cairo1_contract_class_hash),
            (cairo1_contract_address_b, cairo1_contract_class_hash),
            (cairo0_contract_address_a, cairo0_contract_class_hash),
            (cairo0_contract_address_b, cairo0_contract_class_hash),
        ]);
        let is_cairo1 = BTreeMap::from([
            (cairo1_replacement_class_hash, true),
            (cairo1_contract_class_hash, true),
            (cairo0_contract_class_hash, false),
        ]);

        // Initialize the fuzz testing contracts with the orchestrator address.
        for address in deployed_fuzz_contracts.keys() {
            let calldata =
                create_calldata(*address, "initialize", &vec![**orchestrator_contract_address]);
            test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });
        }

        // First call is the orchestrator calling the first fuzz test contract.
        let first_call = FuzzCallInfo::new_call(
            cairo1_contract_address_a,
            cairo1_contract_class_hash,
            // The orchestrator always starts the test in a catching context.
            ParentFailureBehavior::Cairo1Catching,
        );
        Self {
            calls: vec![first_call],
            current_call: vec![0],
            final_state: FinalizedState::Ongoing,
            operations: vec![],
            deployed_contracts: deployed_fuzz_contracts,
            cairo1_replacement_class_hash,
            class_replaced: false,
            is_cairo1,
            valid_storage_keys: vec![Felt::from(1u16 << 8), Felt::from(1u16 << 9)],
            next_storage_write_value: StarknetStorageValue(Felt::from(1u16 << 12)),
            next_salt: ContractAddressSalt(Felt::from(1u32 << 16)),
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
        let current_call = self.current_call.clone();
        self.fuzz_call_info_mut(&current_call)
    }

    pub fn fuzz_call_info_mut(&mut self, call_path: &Vec<usize>) -> &mut FuzzCallInfo {
        let mut call = &mut self.calls[call_path[0]];
        for i in 1..call_path.len() {
            call = &mut call.inner_calls[call_path[i]];
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
        *self.is_cairo1.get(class_hash).unwrap()
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
                self.deployed_contracts
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
                self.is_cairo1
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
            FuzzOperation::Write => {
                // We have two storage keys to choose from.
                self.valid_storage_keys
                    .iter()
                    .map(|storage_key| {
                        FuzzOperationData::Write(
                            StorageKey::try_from(*storage_key).unwrap(),
                            self.next_storage_write_value,
                        )
                    })
                    .collect()
            }
            FuzzOperation::ReplaceClass => {
                // If class was already replaced, no more replacements are allowed.
                if self.class_replaced {
                    return vec![];
                }
                vec![FuzzOperationData::ReplaceClass(self.cairo1_replacement_class_hash)]
            }
            FuzzOperation::Deploy => {
                // Three class hashes to choose from.
                self.is_cairo1
                    .keys()
                    .map(|class_hash| FuzzOperationData::Deploy {
                        class_hash: *class_hash,
                        salt: self.next_salt,
                    })
                    .collect()
            }
            FuzzOperation::Panic => vec![FuzzOperationData::Panic],
        }
    }

    /// List of all valid single operations that can be applied on the current context.
    pub fn valid_operations(&self) -> Vec<FuzzOperationData> {
        FuzzOperation::iter().flat_map(|op| self.valid_operations_of_type(op)).collect()
    }

    /// Enter a new call or deploy context.
    fn enter(
        &mut self,
        address: ContractAddress,
        class_hash: ClassHash,
        parent_failure_behavior: ParentFailureBehavior,
        constructor: bool,
    ) {
        let current_call = self.current_fuzz_call_info_mut();
        let next_call_index = current_call.inner_calls.len();
        if constructor {
            assert_eq!(parent_failure_behavior, ParentFailureBehavior::Uncatchable);
            current_call.inner_calls.push(FuzzCallInfo::new_deploy(address, class_hash));
        } else {
            current_call.inner_calls.push(FuzzCallInfo::new_call(
                address,
                class_hash,
                parent_failure_behavior,
            ));
        }
        self.current_call.push(next_call_index);
    }

    /// Enter a new call context.
    pub fn enter_call(
        &mut self,
        address: ContractAddress,
        class_hash: ClassHash,
        parent_failure_behavior: ParentFailureBehavior,
    ) {
        self.enter(address, class_hash, parent_failure_behavior, false);
    }

    /// Enter a new deploy (constructor) context.
    pub fn enter_deploy(&mut self, address: ContractAddress, class_hash: ClassHash) {
        self.enter(address, class_hash, ParentFailureBehavior::Uncatchable, true);
    }

    /// Exit the current call context.
    pub fn exit_call(&mut self) {
        self.current_call.pop();
        // If we returned to orchestrator context, no more operations can be applied.
        if self.current_call.is_empty() {
            self.final_state = FinalizedState::Succeeded;
        }
    }

    /// Expected address of a deploy operation.
    pub fn address_of_deploy(
        &self,
        class_hash: ClassHash,
        salt: ContractAddressSalt,
    ) -> ContractAddress {
        // Orchestrator address, and boolean to indicate the fuzz test should run.
        let ctor_calldata = calldata![**self.orchestrator_contract_address, Felt::ONE];
        // Deployer address is always zero (for simplicity).
        calculate_contract_address(salt, class_hash, &ctor_calldata, ContractAddress::default())
            .unwrap()
    }

    /// Given a call path, revert all the context changes induced by it and it's child calls.
    pub fn compute_revert_info(&self, root_call: &FuzzCallInfo) -> RevertInfo {
        RevertInfo::combine(
            root_call
                // Start with inner calls.
                .inner_calls
                .iter()
                .map(|call| self.compute_revert_info(call))
                // Add the root call.
                .chain(vec![RevertInfo {
                    deployed_addresses: if root_call.constructing {
                        BTreeSet::from([root_call.address])
                    } else {
                        BTreeSet::new()
                    },
                    class_replaced_and_original_class_hash: if root_call.class_replaced_here {
                        Some((root_call.address, root_call.class_hash))
                    } else {
                        None
                    },
                }])
                .collect::<Vec<_>>(),
        )
    }

    pub fn apply_revert_info(&mut self, revert_info: RevertInfo) {
        // Revert class replacement. Do this before "undeploying" deployed contracts so we don't
        // "redeploy" anything when we only intend to revert the class hash change.
        if let Some((address, class_hash)) = revert_info.class_replaced_and_original_class_hash {
            self.deployed_contracts.insert(address, class_hash);
        }
        // "Undeploy" all deployed contracts.
        for address in revert_info.deployed_addresses.iter() {
            // Remove without asserting that the address was actually deployed - the
            // constructor may have reverted before being finalized.
            self.deployed_contracts.remove(address);
        }
    }

    /// Applies the operation and updates the context.
    pub fn apply(&mut self, operation: FuzzOperationData) {
        assert!(!self.finalized());
        self.operations.push(operation);
        match operation {
            FuzzOperationData::Return => {
                // Go up the call tree.
                self.exit_call()
            }
            FuzzOperationData::Call(call_operation_data) => {
                let address = *call_operation_data.address();
                let class_hash = *self.deployed_contracts.get(&address).unwrap();
                self.enter_call(address, class_hash, call_operation_data.parent_failure_behavior());
            }
            FuzzOperationData::LibraryCall(library_call_operation_data) => {
                let current_address = self.current_address();
                self.enter_call(
                    current_address,
                    *library_call_operation_data.class_hash(),
                    library_call_operation_data.parent_failure_behavior(),
                );
            }
            FuzzOperationData::Write(_, _) => {
                self.next_storage_write_value.0 += Felt::ONE;
            }
            FuzzOperationData::ReplaceClass(class_hash) => {
                assert!(!self.class_replaced);
                assert_eq!(class_hash, self.cairo1_replacement_class_hash);
                self.class_replaced = true;
                // Update the mapping from address to class hash, so subsequent calls to this
                // address will correctly use the new class hash.
                self.deployed_contracts.insert(self.current_address(), class_hash);
                // Update the current call to mark that it was replaced at this point, to make it
                // easy to track if the change must be reverted mid-test.
                self.current_fuzz_call_info_mut().class_replaced_here = true;
                // Note: we do not mutate the class hash of this call, because in the current call
                // context the original class code is run. Only if this address is called again (via
                // call-contract) should the code change be reflected.
            }
            FuzzOperationData::Deploy { class_hash, salt } => {
                let deployed_address = self.address_of_deploy(class_hash, salt);
                // Increment the salt for the next deploy operation.
                self.next_salt.0 += Felt::ONE;
                // Update the mapping from address to class hash.
                self.deployed_contracts.insert(deployed_address, class_hash);
                // Enter constructor context.
                self.enter_deploy(deployed_address, class_hash);
            }
            FuzzOperationData::Panic => {
                // For the current call index until the panic is either caught or an uncatchable
                // frame is reached (root parent frame - the orchestrator - is cairo1-catching, so
                // one of these two conditions will be met).
                // First, check if the current call is in cairo0 context. If so, parent context is
                // irrelevant - the entire tx will be reverted.
                if !self.is_cairo1_class(&self.current_class_hash()) {
                    self.final_state = FinalizedState::Reverted;
                    return;
                }
                // Otherwise, climb up the call tree until the error is either caught or an
                // uncatchable frame is reached.
                while self.current_fuzz_call_info().parent_failure_behavior
                    == ParentFailureBehavior::Cairo1Propagating
                {
                    // No need to finalize deploys here - we are reverting.
                    self.current_call.pop();
                }
                match self.current_fuzz_call_info().parent_failure_behavior {
                    // The simple case is when the parent is "uncatchable"; the entire tx will be
                    // reverted, so no need to update the current context.
                    ParentFailureBehavior::Uncatchable => {
                        self.final_state = FinalizedState::Reverted;
                    }
                    // If the panic is caught, the effects of the entire subtree must be reverted.
                    ParentFailureBehavior::Cairo1Catching => {
                        // Revert the effects of the call tree rooted at the current path.
                        self.apply_revert_info(
                            self.compute_revert_info(self.current_fuzz_call_info()),
                        );
                        // Pop the current call index to go back up to the catching context.
                        self.exit_call();
                    }
                    ParentFailureBehavior::Cairo1Propagating => unreachable!(),
                }
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
        let operation = valid_operations.into_iter().choose(&mut self.rng).unwrap();
        self.apply(operation);
        Ok(())
    }

    /// Convert the list of operations to a vector of felt values that can be used as calldata for a
    /// fuzz test.
    pub fn operations_to_scenario_data(operations: &Vec<FuzzOperationData>) -> Vec<Felt> {
        operations.into_iter().flat_map(|op| op.to_felt_vector()).collect()
    }

    /// Pretty print the operations. Example output:
    /// ```ignore
    /// operations = [
    ///     0x1 (Call),
    ///     0xdeadbeef (Cairo1 address, class hash: 0xbeef),
    ///     0x1 (unwraps error),
    ///     0x2 (Library call),
    ///     0xbee (Cairo0 class hash),
    ///     0x0 (does not unwrap error),
    ///     0x1 (Call),
    ///     0xdeadbeef (Cairo1 address, class hash: 0xbeef),
    ///     0x0 (Return),
    ///     0x6 (Panic),
    /// ]
    /// ```
    #[allow(unused)]
    pub fn prettify_operations(&self) -> String {
        let mut output = vec!["operations = [".to_string()];
        for operation in self.operations.iter() {
            let operation_felt_hexes = operation
                .to_felt_vector()
                .iter()
                .map(|felt| felt.to_hex_string())
                .collect::<Vec<String>>();
            output.extend(match operation {
                FuzzOperationData::Return => vec![format!("{} (Return)", operation_felt_hexes[0])],
                FuzzOperationData::Call(call_operation_data) => {
                    let class_hash =
                        self.deployed_contracts.get(call_operation_data.address()).unwrap();
                    let is_cairo1 = self.is_cairo1_class(class_hash);
                    let mut call_print = vec![
                        format!("{} (Call)", operation_felt_hexes[0]),
                        format!(
                            "{} (Cairo{} address, class hash: {})",
                            operation_felt_hexes[1],
                            if is_cairo1 { "1" } else { "0" },
                            class_hash.0.to_hex_string()
                        ),
                    ];
                    if let CallOperationData::Cairo1 { unwraps_error, .. } = call_operation_data {
                        call_print.push(format!(
                            "{} ({} error)",
                            operation_felt_hexes[2],
                            if *unwraps_error { "unwraps" } else { "does not unwrap" }
                        ));
                    }
                    call_print
                }
                FuzzOperationData::LibraryCall(library_call_operation_data) => {
                    let is_cairo1 = self.is_cairo1_class(library_call_operation_data.class_hash());
                    let mut library_call_print = vec![
                        format!("{} (Library call)", operation_felt_hexes[0]),
                        format!(
                            "{} (Cairo{} class hash)",
                            operation_felt_hexes[1],
                            if is_cairo1 { "1" } else { "0" },
                        ),
                    ];
                    if let LibraryCallOperationData::Cairo1 { unwraps_error, .. } =
                        library_call_operation_data
                    {
                        library_call_print.push(format!(
                            "{} ({} error)",
                            operation_felt_hexes[2],
                            if *unwraps_error { "unwraps" } else { "does not unwrap" }
                        ));
                    }
                    library_call_print
                }
                FuzzOperationData::Write(_, _) => {
                    vec![
                        format!("{} (Write)", operation_felt_hexes[0]),
                        format!("{} (key)", operation_felt_hexes[1]),
                        format!("{} (value)", operation_felt_hexes[2]),
                    ]
                }
                FuzzOperationData::ReplaceClass(_) => {
                    vec![
                        format!("{} (Replace class)", operation_felt_hexes[0]),
                        format!("{} (new class hash)", operation_felt_hexes[1]),
                    ]
                }
                FuzzOperationData::Deploy { class_hash, salt } => {
                    let deployed_address = self.address_of_deploy(*class_hash, *salt);
                    let is_cairo1 = self.is_cairo1_class(class_hash);
                    vec![
                        format!(
                            "{} (Deploy, new address: {})",
                            operation_felt_hexes[0],
                            deployed_address.to_hex_string()
                        ),
                        format!(
                            "{} (Cairo{} class hash)",
                            operation_felt_hexes[1],
                            if is_cairo1 { "1" } else { "0" }
                        ),
                        format!("{} (salt)", operation_felt_hexes[2]),
                    ]
                }
                FuzzOperationData::Panic => {
                    vec![format!("{} (Panic)", operation_felt_hexes[0])]
                }
            });
        }
        for line in output.iter_mut().skip(1) {
            *line = format!("    {line}");
        }
        output.push("]".to_string());
        output.join(",\n").to_string()
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
            FinalizedState::Reverted => Some("".to_string()),
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

async fn fuzz_test_body(seed: u64, max_n_operations: usize) {
    let mut fuzz_tester = FuzzTestManager::init(seed).await;

    // Create scenarios.
    for _ in 0..max_n_operations {
        // An error value means the context is finalized - no more operations can be applied.
        if let Err(_) = fuzz_tester.add_random_operation() {
            break;
        }
    }

    println!("Seed: {seed}.");
    #[cfg(feature = "fuzz_test_debug")]
    println!("{}", fuzz_tester.prettify_operations());

    fuzz_tester.run_test().await;
}

#[rstest]
#[tokio::test]
async fn test_daily_fuzz_seed(
    #[values(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15)] inner_seed: u64,
) {
    let now = Utc::now();
    let day: u64 = now.day().into();
    let month: u64 = now.month().into();
    let year: u64 = now.year().try_into().unwrap();
    let seed = day * 100000000 + month * 1000000 + year * 100 + inner_seed;
    fuzz_test_body(seed, 10).await;
}

#[cfg(feature = "long_fuzz_test")]
mod long_fuzz_test {
    use super::*;

    /// Long fuzz test. This generates a lot of code, so instead of `#[cfg_attr(.., ignore)]`, we
    /// gate the actual module.
    /// It is strongly recommended to run this test in release mode only.
    #[rstest]
    #[tokio::test]
    async fn test_cairo1_revert_fuzz(
        #[values(0, 1, 2, 3, 4, 5, 6, 7, 8, 9)] seed0: u64,
        #[values(0, 1, 2, 3, 4, 5, 6, 7, 8, 9)] seed1: u64,
        #[values(0, 1, 2, 3, 4, 5, 6, 7, 8, 9)] seed2: u64,
        #[values(0, 1, 2, 3, 4, 5, 6, 7, 8, 9)] seed3: u64,
    ) {
        let seed_base = 10;
        assert!(seed0 < seed_base);
        assert!(seed1 < seed_base);
        assert!(seed2 < seed_base);
        assert!(seed3 < seed_base);
        let seed = seed0
            + seed1 * seed_base
            + seed2 * seed_base * seed_base
            + seed3 * seed_base * seed_base * seed_base;
        fuzz_test_body(seed, 10).await;
    }
}
