use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use blockifier::test_utils::dict_state_reader::DictStateReader;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::calldata::create_calldata;
use blockifier_test_utils::contracts::FeatureContract;
use chrono::{Datelike, Utc};
use expect_test::{expect, Expect};
use itertools::Itertools;
use rand::prelude::IteratorRandom;
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha8Rng;
use rstest::rstest;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::core::{
    calculate_contract_address,
    ClassHash,
    ContractAddress,
    EntryPointSelector,
};
use starknet_api::state::StorageKey;
use starknet_api::transaction::fields::ContractAddressSalt;
use starknet_api::transaction::{L2ToL1Payload, MessageToL1};
use starknet_api::{calldata, felt, invoke_tx_args};
use starknet_committer::block_committer::input::StarknetStorageValue;
use starknet_types_core::felt::Felt;
use strum::{EnumIter, IntoEnumIterator};
use tokio::task::JoinSet;

use crate::test_manager::{TestBuilder, FUNDED_ACCOUNT_ADDRESS};
use crate::tests::NON_TRIVIAL_RESOURCE_BOUNDS;
use crate::utils::get_class_hash_of_feature_contract;

/// Maximum length of operation lists to test exhaustively.
const MAX_EXHAUSTIVE_FUZZ_LENGTH: usize = 3;

/// Number of exhaustive fuzz test tasks to spawn in parallel. As long as this is at least the
/// number of cores, all cores will be utilized.
const NUM_EXHAUSTIVE_PARALLEL_FUZZ_TESTS: usize = 100;

/// Entry point selector of main (recursive) fuzzing function.
static FUZZ_ENTRY_POINT: LazyLock<EntryPointSelector> =
    LazyLock::new(|| selector_from_name("test_revert_fuzz"));
/// Dummy (undeployed) contract address.
static UNDEPLOYED_CONTRACT_ADDRESS: LazyLock<ContractAddress> =
    LazyLock::new(|| ContractAddress::try_from(felt!("0xdeedee")).unwrap());

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
static IS_CAIRO1: LazyLock<BTreeMap<ClassHash, bool>> = LazyLock::new(|| {
    BTreeMap::from([
        (*CAIRO0_CONTRACT_CLASS_HASH, false),
        (*CAIRO1_CONTRACT_CLASS_HASH, true),
        (*CAIRO1_REPLACEMENT_CLASS_HASH, true),
    ])
});

/// Initial fuzz contract addresses.
static FUZZ_ADDRESS_ORCHESTRATOR_EXPECT: Expect =
    expect!["0x19f4866af3211922b1f169d754c551042c8840305c62edef20fa1c3925246f4"];
static FUZZ_ADDRESS_CAIRO1_A_EXPECT: Expect =
    expect!["0x63310958764da43bd2bfe2e9bc0d67dd4f461221b99036264b9d2f9a9886a25"];
static FUZZ_ADDRESS_CAIRO1_B_EXPECT: Expect =
    expect!["0x3e93601f7a280a91f3dedfea0a4788a95bd6e5081417150978f40a26ebdb725"];
static FUZZ_ADDRESS_CAIRO0_A_EXPECT: Expect =
    expect!["0x71fdbf189fc7367aeaf569b481798c81430641bdc679ea9ad4e0cf410332969"];
static FUZZ_ADDRESS_CAIRO0_B_EXPECT: Expect =
    expect!["0x1656a98eb22ad62fdb95cc2aef0ad6c35e4d15237d3fb1e925f12d20023f77c"];
static FUZZ_ADDRESS_ORCHESTRATOR: LazyLock<ContractAddress> = LazyLock::new(|| {
    ContractAddress::try_from(felt!(FUZZ_ADDRESS_ORCHESTRATOR_EXPECT.data())).unwrap()
});
static FUZZ_ADDRESS_CAIRO1_A: LazyLock<ContractAddress> = LazyLock::new(|| {
    ContractAddress::try_from(felt!(FUZZ_ADDRESS_CAIRO1_A_EXPECT.data())).unwrap()
});
static FUZZ_ADDRESS_CAIRO1_B: LazyLock<ContractAddress> = LazyLock::new(|| {
    ContractAddress::try_from(felt!(FUZZ_ADDRESS_CAIRO1_B_EXPECT.data())).unwrap()
});
static FUZZ_ADDRESS_CAIRO0_A: LazyLock<ContractAddress> = LazyLock::new(|| {
    ContractAddress::try_from(felt!(FUZZ_ADDRESS_CAIRO0_A_EXPECT.data())).unwrap()
});
static FUZZ_ADDRESS_CAIRO0_B: LazyLock<ContractAddress> = LazyLock::new(|| {
    ContractAddress::try_from(felt!(FUZZ_ADDRESS_CAIRO0_B_EXPECT.data())).unwrap()
});
static FUZZ_ADDRESS_TO_CLASS_HASH: LazyLock<BTreeMap<ContractAddress, ClassHash>> =
    LazyLock::new(|| {
        BTreeMap::from([
            (*FUZZ_ADDRESS_CAIRO1_A, *CAIRO1_CONTRACT_CLASS_HASH),
            (*FUZZ_ADDRESS_CAIRO1_B, *CAIRO1_CONTRACT_CLASS_HASH),
            (*FUZZ_ADDRESS_CAIRO0_A, *CAIRO0_CONTRACT_CLASS_HASH),
            (*FUZZ_ADDRESS_CAIRO0_B, *CAIRO0_CONTRACT_CLASS_HASH),
        ])
    });

/// Storage key that can be written to.
static VALID_STORAGE_KEYS: LazyLock<Vec<Felt>> =
    LazyLock::new(|| vec![Felt::from(1u16 << 8), Felt::from(1u16 << 9)]);

/// Filter functions for custom scenario filters.
type OperationFilter = fn(&FuzzOperationData) -> bool;
/// All scenarios allowed.
fn op_filter_all(_: &FuzzOperationData) -> bool {
    true
}
/// Call, write, panic, return scenarios only.
fn op_filter_call_write_panic_return(op: &FuzzOperationData) -> bool {
    matches!(
        op,
        FuzzOperationData::Call(_)
            | FuzzOperationData::Return
            | FuzzOperationData::Write(_, _)
            | FuzzOperationData::Panic
    )
}
const OP_FILTER_CALL_WRITE_PANIC_RETURN: OperationFilter = op_filter_call_write_panic_return;

// TODO(Dori): Operations to add:
// 3. events
// 4. call / libcall non-existing entry points (should panic) (catchable in cairo0 even?)
#[derive(Clone, Copy, EnumIter)]
enum FuzzOperation {
    Return,
    Call,
    LibraryCall,
    Write,
    ReplaceClass,
    Deploy,
    Panic,
    IncrementCounter,
    SendMessage,
    DeployNonexisting,
    LibraryCallNonexistingClass,
    Sha256,
    Keccak,
    CallUndeployed,
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
            Self::IncrementCounter => 7u8,
            Self::SendMessage => 8u8,
            Self::DeployNonexisting => 9u8,
            Self::LibraryCallNonexistingClass => 10u8,
            Self::Sha256 => 11u8,
            Self::Keccak => 12u8,
            Self::CallUndeployed => 13u8,
        })
    }
}

#[derive(Clone, Copy, Debug)]
struct CallOperationData {
    pub from_cairo1: bool,
    pub address: ContractAddress,
    pub selector: EntryPointSelector,
    pub unwraps_error: bool,
}

impl CallOperationData {
    pub fn felt_vector(&self) -> Vec<Felt> {
        vec![**self.address, self.selector.0, self.unwraps_error.into()]
    }

    pub fn parent_failure_behavior(&self) -> ParentFailureBehavior {
        if self.from_cairo1 {
            ParentFailureBehavior::cairo1_behavior(self.unwraps_error)
        } else {
            ParentFailureBehavior::cairo0_behavior()
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct LibraryCallOperationData {
    pub from_cairo1: bool,
    pub class_hash: ClassHash,
    pub selector: EntryPointSelector,
    pub unwraps_error: bool,
}

impl LibraryCallOperationData {
    pub fn felt_vector(&self) -> Vec<Felt> {
        vec![self.class_hash.0, self.selector.0, self.unwraps_error.into()]
    }

    pub fn parent_failure_behavior(&self) -> ParentFailureBehavior {
        if self.from_cairo1 {
            ParentFailureBehavior::cairo1_behavior(self.unwraps_error)
        } else {
            ParentFailureBehavior::cairo0_behavior()
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
    IncrementCounter,
    SendMessage(Felt),
    DeployNonexisting,
    LibraryCallNonexistingClass,
    Sha256(Felt),
    Keccak(Felt),
    CallUndeployed(CallOperationData),
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
            Self::IncrementCounter => FuzzOperation::IncrementCounter,
            Self::SendMessage(_) => FuzzOperation::SendMessage,
            Self::DeployNonexisting => FuzzOperation::DeployNonexisting,
            Self::LibraryCallNonexistingClass => FuzzOperation::LibraryCallNonexistingClass,
            Self::Sha256(_) => FuzzOperation::Sha256,
            Self::Keccak(_) => FuzzOperation::Keccak,
            Self::CallUndeployed(_) => FuzzOperation::CallUndeployed,
        }
    }

    /// Convert the operation data to a vector of felt values that can be used as calldata for a
    /// fuzz test.
    pub fn felt_vector(&self) -> Vec<Felt> {
        let mut felt_vector = vec![self.op().identifier()];
        felt_vector.extend(match self {
            Self::Return
            | Self::Panic
            | Self::IncrementCounter
            | Self::DeployNonexisting
            | Self::LibraryCallNonexistingClass => vec![],
            Self::Call(op) | Self::CallUndeployed(op) => op.felt_vector(),
            Self::LibraryCall(op) => op.felt_vector(),
            Self::Write(storage_key, value) => vec![***storage_key, value.0],
            Self::ReplaceClass(class_hash) => vec![class_hash.0],
            Self::Deploy { class_hash, salt } => vec![class_hash.0, salt.0],
            Self::SendMessage(message) => vec![*message],
            Self::Sha256(value) | Self::Keccak(value) => vec![*value],
        });
        felt_vector
    }
}

/// Parent frame behavior on failures.
#[derive(Clone, Debug, PartialEq)]
enum ParentFailureBehavior {
    /// In a cairo0 context, or in a constructor call tree. Failures in this context cannot be
    /// caught by any calling context.
    Uncatchable,

    /// Not unwrapping errors from child context.
    Catching,

    /// In cairo1, unwrapping errors from next context.
    /// Propagating errors is only possible from Cairo1 context.
    Cairo1Propagating,
}

impl ParentFailureBehavior {
    pub fn cairo1_behavior(unwraps_error: bool) -> Self {
        if unwraps_error { Self::Cairo1Propagating } else { Self::Catching }
    }

    pub fn cairo0_behavior() -> Self {
        Self::Uncatchable
    }
}

/// Final state of the fuzz test transaction.
#[derive(Clone)]
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
#[derive(Clone, Debug)]
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
    /// Payloads of messages sent to L1 in this call.
    pub messages: Vec<Felt>,
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
            messages: vec![],
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
            messages: vec![],
        }
    }
}

/// Data associated with a revert operation.
struct RevertInfo {
    /// Addresses of deployed contracts that were deployed in the reverted call tree.
    pub deployed_addresses: BTreeSet<ContractAddress>,

    /// If a class was replaced in the reverted call tree, the original class hash of the replaced
    /// class.
    pub class_replaced: Option<ClassHash>,
}

impl RevertInfo {
    pub fn combine(others: Vec<Self>) -> Self {
        // If there is a replace-class, there should be only one.
        let mut class_replaced = None;
        for other in others.iter() {
            if other.class_replaced.is_some() {
                assert!(class_replaced.is_none());
                class_replaced = other.class_replaced;
            }
        }
        Self {
            deployed_addresses: others
                .into_iter()
                .flat_map(|other| other.deployed_addresses)
                .collect(),
            class_replaced,
        }
    }
}

/// Represents the call tree of a fuzz test.
#[derive(Clone)]
struct FuzzTestContext {
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

    /// Map from newly deployed contract address to class hash.
    pub newly_deployed_contracts: BTreeMap<ContractAddress, ClassHash>,

    /// We only allow one replace class per test. Track which contract was replaced.
    pub replaced_address: Option<ContractAddress>,

    /// Next value to write in a storage-write operation.
    pub next_storage_write_value: StarknetStorageValue,

    /// Contents of the next message to send.
    pub next_message: Felt,

    /// Next salt to use for a deploy operation.
    pub next_salt: ContractAddressSalt,

    /// Next hash preimage to use for a hash operation.
    pub next_hash_preimage: Felt,

    pub rng: ChaCha8Rng,
}

impl FuzzTestContext {
    pub fn init(seed: u64, first_call: FuzzCallInfo) -> Self {
        Self {
            calls: vec![first_call],
            current_call: vec![0],
            final_state: FinalizedState::Ongoing,
            operations: vec![],
            newly_deployed_contracts: BTreeMap::new(),
            replaced_address: None,
            next_storage_write_value: StarknetStorageValue(Felt::from(1u16 << 12)),
            next_message: Felt::from(1u32 << 20),
            next_salt: ContractAddressSalt(Felt::from(1u32 << 16)),
            next_hash_preimage: Felt::ONE,
            rng: ChaCha8Rng::seed_from_u64(seed),
        }
    }

    pub fn finalized(&self) -> bool {
        self.final_state.finalized()
    }

    pub fn current_fuzz_call_info(&self) -> &FuzzCallInfo {
        let mut call = &self.calls[self.current_call[0]];
        for index in self.current_call.iter().skip(1) {
            call = &call.inner_calls[*index];
        }
        call
    }

    pub fn current_fuzz_call_info_mut(&mut self) -> &mut FuzzCallInfo {
        let current_call = self.current_call.clone();
        self.fuzz_call_info_mut(&current_call)
    }

    pub fn fuzz_call_info_mut(&mut self, call_path: &[usize]) -> &mut FuzzCallInfo {
        let mut call = &mut self.calls[call_path[0]];
        for index in call_path.iter().skip(1) {
            call = &mut call.inner_calls[*index];
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

    pub fn deployed_contracts(&self) -> impl Iterator<Item = &ContractAddress> {
        self.newly_deployed_contracts.keys().chain(FUZZ_ADDRESS_TO_CLASS_HASH.keys())
    }

    pub fn try_class_hash_of(&self, address: &ContractAddress) -> Option<ClassHash> {
        match self.replaced_address {
            Some(replaced_address) if &replaced_address == address => {
                Some(*CAIRO1_REPLACEMENT_CLASS_HASH)
            }
            _ => FUZZ_ADDRESS_TO_CLASS_HASH
                .get(address)
                .copied()
                .or_else(|| self.newly_deployed_contracts.get(address).copied()),
        }
    }

    pub fn class_hash_of(&self, address: &ContractAddress) -> ClassHash {
        self.try_class_hash_of(address).unwrap()
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
                // Cairo0 always "unwraps" errors.
                let from_cairo1 = self.is_current_context_cairo1();
                let possible_unwraps_errors =
                    if from_cairo1 { vec![true, false] } else { vec![true] };
                self.deployed_contracts()
                    .flat_map(|address| {
                        possible_unwraps_errors.iter().copied().map(|unwraps_error| {
                            FuzzOperationData::Call(CallOperationData {
                                from_cairo1,
                                address: *address,
                                selector: *FUZZ_ENTRY_POINT,
                                unwraps_error,
                            })
                        })
                    })
                    .collect()
            }
            FuzzOperation::LibraryCall => {
                // We have one Cairo0 contract and two Cairo1 contracts to choose from.
                // Similar to calls, when calling from a Cairo1 context, the caller can unwrap the
                // call result or not. Cairo0 always "unwraps" errors.
                let from_cairo1 = self.is_current_context_cairo1();
                let possible_unwraps_errors =
                    if from_cairo1 { vec![true, false] } else { vec![true] };
                IS_CAIRO1
                    .keys()
                    .flat_map(|class_hash| {
                        possible_unwraps_errors.iter().copied().map(|unwraps_error| {
                            FuzzOperationData::LibraryCall(LibraryCallOperationData {
                                from_cairo1,
                                class_hash: *class_hash,
                                selector: *FUZZ_ENTRY_POINT,
                                unwraps_error,
                            })
                        })
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
                if self.replaced_address.is_some() {
                    // TODO(Dori): In this case, replace back to original class.
                    return vec![];
                }
                vec![FuzzOperationData::ReplaceClass(*CAIRO1_REPLACEMENT_CLASS_HASH)]
            }
            FuzzOperation::Deploy => {
                // Three class hashes to choose from.
                IS_CAIRO1
                    .keys()
                    .map(|class_hash| FuzzOperationData::Deploy {
                        class_hash: *class_hash,
                        salt: self.next_salt,
                    })
                    .collect()
            }
            FuzzOperation::Panic => vec![FuzzOperationData::Panic],
            FuzzOperation::IncrementCounter => vec![FuzzOperationData::IncrementCounter],
            FuzzOperation::SendMessage => vec![FuzzOperationData::SendMessage(self.next_message)],
            FuzzOperation::DeployNonexisting => vec![FuzzOperationData::DeployNonexisting],
            FuzzOperation::LibraryCallNonexistingClass => {
                vec![FuzzOperationData::LibraryCallNonexistingClass]
            }
            FuzzOperation::Sha256 => {
                // Syscall only exists in Cairo1.
                if self.is_current_context_cairo1() {
                    vec![FuzzOperationData::Sha256(self.next_hash_preimage)]
                } else {
                    vec![]
                }
            }
            FuzzOperation::Keccak => {
                // Syscall only exists in Cairo1.
                if self.is_current_context_cairo1() {
                    vec![FuzzOperationData::Keccak(self.next_hash_preimage)]
                } else {
                    vec![]
                }
            }
            FuzzOperation::CallUndeployed => {
                // Neither Cairo1 nor Cairo0 contexts can catch an error of this type.
                vec![FuzzOperationData::CallUndeployed(CallOperationData {
                    from_cairo1: self.is_current_context_cairo1(),
                    address: *UNDEPLOYED_CONTRACT_ADDRESS,
                    selector: *FUZZ_ENTRY_POINT,
                    unwraps_error: true,
                })]
            }
        }
    }

    fn valid_filtered_operations(&self, filter: OperationFilter) -> Vec<FuzzOperationData> {
        FuzzOperation::iter()
            .flat_map(|op| self.valid_operations_of_type(op))
            .filter(filter)
            .collect()
    }

    /// List of all valid single operations that can be applied on the current context.
    pub fn valid_operations(&self, filter: Option<OperationFilter>) -> Vec<FuzzOperationData> {
        self.valid_filtered_operations(filter.unwrap_or(op_filter_all))
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
    pub fn address_of_deploy(class_hash: ClassHash, salt: ContractAddressSalt) -> ContractAddress {
        // Deployer address is always zero (for simplicity).
        calculate_contract_address(
            salt,
            class_hash,
            &calldata![***FUZZ_ADDRESS_ORCHESTRATOR],
            ContractAddress::default(),
        )
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
                    class_replaced: if root_call.class_replaced_here {
                        Some(root_call.class_hash)
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
        if let Some(original_class_hash) = revert_info.class_replaced {
            let replaced_address = self.replaced_address.take().unwrap();
            if self.newly_deployed_contracts.contains_key(&replaced_address) {
                self.newly_deployed_contracts.insert(replaced_address, original_class_hash);
            }
        }
        // "Undeploy" all deployed contracts.
        for address in revert_info.deployed_addresses.iter() {
            // Remove without asserting that the address was actually deployed - the
            // constructor may have reverted before being finalized.
            self.newly_deployed_contracts.remove(address);
        }
    }

    /// Remove the entire call tree. Used when an uncatchable error occurs, or we cleanly return
    /// to the orchestrator context.
    /// State should always be finalized after this.
    pub fn pop_entire_call_tree(&mut self, succeeded: bool) {
        self.calls.clear();
        self.current_call.clear();
        self.final_state =
            if succeeded { FinalizedState::Succeeded } else { FinalizedState::Reverted };
    }

    /// Update the context to reflect the effects of a panic in the current call.
    pub fn apply_panic(&mut self) {
        // For the current call index until the panic is either caught or an uncatchable frame is
        // reached (root parent frame - the orchestrator - is cairo1-catching, so one of these two
        // conditions will be met).
        // First, check if the current call is in cairo0 context. If so, parent context is
        // irrelevant - the entire tx will be reverted.
        if !self.is_cairo1_class(&self.current_class_hash()) {
            self.pop_entire_call_tree(false);
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
                self.pop_entire_call_tree(false);
            }
            // If the panic is caught, the effects of the entire subtree must be reverted.
            ParentFailureBehavior::Catching => {
                // Revert the effects of the call tree rooted at the current path.
                self.apply_revert_info(self.compute_revert_info(self.current_fuzz_call_info()));
                // Pop the current call index to go back up to the catching context.
                self.exit_call();
                // Pop the reverted call frame from the call tree. Example scenario for why
                // this is needed:
                // 1. non-unwrapping call
                // 2. replace class
                // 3. panic
                // 4. panic
                // The first panic is caught and will revert the replace class. Unless the
                // inner call is popped, the second panic will attempt to revert the replace
                // class again.
                if self.current_call.is_empty() {
                    // We are back at the orchestrator context. Pop the entire call tree.
                    // Tx should be successful.
                    self.pop_entire_call_tree(true);
                } else {
                    // We are back at a non-orchestrator context. Pop the last inner call.
                    self.current_fuzz_call_info_mut().inner_calls.pop();
                }
            }
            ParentFailureBehavior::Cairo1Propagating => unreachable!(),
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
                let address = call_operation_data.address;
                let class_hash = self.class_hash_of(&address);
                self.enter_call(address, class_hash, call_operation_data.parent_failure_behavior());
            }
            FuzzOperationData::LibraryCall(library_call_operation_data) => {
                let current_address = self.current_address();
                self.enter_call(
                    current_address,
                    library_call_operation_data.class_hash,
                    library_call_operation_data.parent_failure_behavior(),
                );
            }
            FuzzOperationData::Write(_, _) => {
                self.next_storage_write_value.0 += Felt::ONE;
            }
            FuzzOperationData::ReplaceClass(class_hash) => {
                assert!(self.replaced_address.is_none());
                assert_eq!(class_hash, *CAIRO1_REPLACEMENT_CLASS_HASH);
                let current_address = self.current_address();
                self.replaced_address = Some(current_address);
                // Update the current call to mark that it was replaced at this point, to make it
                // easy to track if the change must be reverted mid-test.
                self.current_fuzz_call_info_mut().class_replaced_here = true;
                // Note: we do not mutate the class hash of this call, because in the current call
                // context the original class code is run. Only if this address is called again (via
                // call-contract) should the code change be reflected.
            }
            FuzzOperationData::Deploy { class_hash, salt } => {
                let deployed_address = Self::address_of_deploy(class_hash, salt);
                // Increment the salt for the next deploy operation.
                self.next_salt.0 += Felt::ONE;
                // Update the mapping from address to class hash.
                self.newly_deployed_contracts.insert(deployed_address, class_hash);
                // Enter constructor context.
                self.enter_deploy(deployed_address, class_hash);
            }
            FuzzOperationData::Panic => self.apply_panic(),
            FuzzOperationData::IncrementCounter => {}
            FuzzOperationData::SendMessage(message) => {
                self.current_fuzz_call_info_mut().messages.push(message);
                self.next_message += Felt::ONE;
            }
            FuzzOperationData::DeployNonexisting
            | FuzzOperationData::LibraryCallNonexistingClass => {
                // Unrecoverable error (we do not prove class hashes do not exist).
                self.pop_entire_call_tree(false);
            }
            FuzzOperationData::Sha256(_) | FuzzOperationData::Keccak(_) => {
                self.next_hash_preimage += Felt::ONE;
            }
            FuzzOperationData::CallUndeployed(_) => {
                // Unrecoverable error (we do not prove addresses are not initialized).
                self.pop_entire_call_tree(false);
            }
        }
    }

    /// Add and apply a random operation.
    /// Returns an error if there are no valid operation to add.
    pub fn add_random_operation(&mut self, filter: Option<OperationFilter>) -> Result<(), ()> {
        let valid_operations = self.valid_operations(filter);
        if valid_operations.is_empty() {
            return Err(());
        }
        let operation = *valid_operations.iter().choose(&mut self.rng).unwrap();
        self.apply(operation);
        Ok(())
    }

    /// Recursive function to generate the expected messages to L1 from a call info.
    /// Messages are not sorted.
    fn expected_messages_to_l1_from_call_info(&self, call_info: &FuzzCallInfo) -> Vec<MessageToL1> {
        let to_address = if self.is_cairo1_class(&call_info.class_hash) {
            Felt::from(0xadd1)
        } else {
            Felt::from(0xadd0)
        }
        .try_into()
        .unwrap();
        call_info
            .messages
            .iter()
            .map(|message| MessageToL1 {
                from_address: call_info.address,
                to_address,
                payload: L2ToL1Payload(vec![*message]),
            })
            .chain(
                call_info
                    .inner_calls
                    .iter()
                    .flat_map(|call| self.expected_messages_to_l1_from_call_info(call)),
            )
            .collect()
    }

    /// Traverses all the fuzz call infos from the root call info and returns the expected messages
    /// to L1.
    /// Order of returned messages is chronological (X is before Y <==> X is sent before Y).
    pub fn expected_messages_to_l1(&self) -> Vec<MessageToL1> {
        match self.calls.first() {
            Some(call) => {
                self.expected_messages_to_l1_from_call_info(call)
                    .into_iter()
                    // Messages are sorted in ascending order of payload.
                    .sorted_by(|message_a, message_b| {
                        Ord::cmp(&message_a.payload.0[0], &message_b.payload.0[0])
                    })
                    .collect()
            }
            None => vec![],
        }
    }

    /// Convert the list of operations to a vector of felt values that can be used as calldata for a
    /// fuzz test.
    pub fn operations_to_scenario_data(&self) -> Vec<Felt> {
        self.operations.iter().flat_map(|op| op.felt_vector()).collect()
    }

    /// Recursive function to generate all possible operation tails of exactly the given length,
    /// given the current context.
    /// WARNING: The number of lists is exponential in the length! Use with small values only.
    /// A value of 4 can generate over 100K lists!
    fn get_all_operation_tails(
        &self,
        max_length: usize,
        filter: Option<OperationFilter>,
    ) -> Vec<Vec<FuzzOperationData>> {
        // Base case.
        if max_length == 0 {
            return vec![self.operations.clone()];
        }
        // We have not reached the target length, but the context is finalized. Skip this scenario.
        if self.finalized() {
            return vec![];
        }
        // Add one operation and recurse.
        self.valid_operations(filter)
            .into_iter()
            .flat_map(|operation| {
                let mut new_context = self.clone();
                new_context.apply(operation);
                new_context.get_all_operation_tails(max_length - 1, filter)
            })
            .collect()
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
    pub fn prettify_operations(&self) -> String {
        let mut output = vec![];
        for operation in self.operations.iter() {
            let operation_felt_hexes = operation
                .felt_vector()
                .iter()
                .map(|felt| felt.to_hex_string())
                .collect::<Vec<String>>();
            output.extend(match operation {
                FuzzOperationData::Return => vec![format!("{} (Return)", operation_felt_hexes[0])],
                FuzzOperationData::Call(CallOperationData {
                    address,
                    selector,
                    unwraps_error,
                    ..
                }) => {
                    // It's possible that the address is no longer deployed (post-revert).
                    let class_info_string = match self.try_class_hash_of(address) {
                        Some(class_hash) => format!(
                            "Cairo{} address, class hash: {}",
                            if self.is_cairo1_class(&class_hash) { "1" } else { "0" },
                            class_hash.0.to_hex_string()
                        ),
                        None => "unknown class hash, deployment reverted".to_string(),
                    };
                    vec![
                        format!("{} (Call)", operation_felt_hexes[0]),
                        format!("{} ({class_info_string})", operation_felt_hexes[1]),
                        format!(
                            "{} (selector, {} function)",
                            operation_felt_hexes[2],
                            if selector == &*FUZZ_ENTRY_POINT { "fuzz" } else { "non existing" }
                        ),
                        format!(
                            "{} ({} error)",
                            operation_felt_hexes[3],
                            if *unwraps_error { "unwraps" } else { "does not unwrap" }
                        ),
                    ]
                }
                FuzzOperationData::LibraryCall(LibraryCallOperationData {
                    class_hash,
                    selector,
                    unwraps_error,
                    ..
                }) => {
                    let is_cairo1 = self.is_cairo1_class(class_hash);
                    vec![
                        format!("{} (Library call)", operation_felt_hexes[0]),
                        format!(
                            "{} (Cairo{} class hash)",
                            operation_felt_hexes[1],
                            if is_cairo1 { "1" } else { "0" },
                        ),
                        format!(
                            "{} (selector, {} function)",
                            operation_felt_hexes[2],
                            if selector == &*FUZZ_ENTRY_POINT { "fuzz" } else { "non existing" }
                        ),
                        format!(
                            "{} ({} error)",
                            operation_felt_hexes[3],
                            if *unwraps_error { "unwraps" } else { "does not unwrap" }
                        ),
                    ]
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
                    let deployed_address = Self::address_of_deploy(*class_hash, *salt);
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
                FuzzOperationData::IncrementCounter => {
                    vec![format!("{} (Increment counter)", operation_felt_hexes[0])]
                }
                FuzzOperationData::SendMessage(_) => {
                    vec![
                        format!("{} (Send message)", operation_felt_hexes[0]),
                        format!("{} (message)", operation_felt_hexes[1]),
                    ]
                }
                FuzzOperationData::DeployNonexisting => {
                    vec![format!("{} (Deploy non-existing)", operation_felt_hexes[0])]
                }
                FuzzOperationData::LibraryCallNonexistingClass => {
                    vec![format!("{} (Library call non-existing class)", operation_felt_hexes[0])]
                }
                FuzzOperationData::Sha256(_) => {
                    vec![
                        format!("{} (Sha256)", operation_felt_hexes[0]),
                        format!("{} (preimage)", operation_felt_hexes[1]),
                    ]
                }
                FuzzOperationData::Keccak(_) => {
                    vec![
                        format!("{} (Keccak)", operation_felt_hexes[0]),
                        format!("{} (preimage)", operation_felt_hexes[1]),
                    ]
                }
                FuzzOperationData::CallUndeployed(call_operation_data) => {
                    vec![
                        format!("{} (Call undeployed)", operation_felt_hexes[0]),
                        format!("{} (dummy address)", operation_felt_hexes[1]),
                        format!("{} (selector)", operation_felt_hexes[2]),
                        format!(
                            "{} ({} error)",
                            operation_felt_hexes[3],
                            if call_operation_data.unwraps_error {
                                "unwraps"
                            } else {
                                "does not unwrap"
                            }
                        ),
                    ]
                }
            });
        }
        format!(
            "operations = [\n{}\n]",
            output.iter().map(|line| format!("    {line}")).collect::<Vec<_>>().join(",\n")
        )
    }
}

/// Manages the fuzz flow test, with the underlying flow test state.
struct FuzzTestManager {
    pub context: FuzzTestContext,
    pub test_manager: TestBuilder<DictStateReader>,
    pub first_called_address: ContractAddress,
}

impl FuzzTestManager {
    pub async fn init(seed: u64) -> Self {
        // Initialize the state with:
        // - an orchestrator contract.
        // - two cairo1 fuzz test contracts.
        // - two cairo0 fuzz test contracts.
        let mut test_manager = Self::init_deployment(false).await;

        // Initialize the fuzz testing contracts with the orchestrator address.
        for address in FUZZ_ADDRESS_TO_CLASS_HASH.keys() {
            let calldata = create_calldata(*address, "initialize", &[***FUZZ_ADDRESS_ORCHESTRATOR]);
            test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });
        }

        // First call is the orchestrator calling the first fuzz test contract.
        let first_called_address = *FUZZ_ADDRESS_CAIRO1_A;
        let first_call = FuzzCallInfo::new_call(
            first_called_address,
            *CAIRO1_CONTRACT_CLASS_HASH,
            // The orchestrator always starts the test in a catching context.
            ParentFailureBehavior::Catching,
        );
        Self {
            context: FuzzTestContext::init(seed, first_call),
            test_manager,
            first_called_address,
        }
    }

    pub async fn init_explicit_test(operations: Vec<FuzzOperationData>) -> Self {
        let mut test_manager = Self::init(0).await;
        for operation in operations {
            test_manager.context.apply(operation);
        }
        test_manager
    }

    /// Initializes the deployment of the fuzz test contracts.
    /// Returns the test builder (after deployment).
    pub async fn init_deployment(assert_expect: bool) -> TestBuilder<DictStateReader> {
        let (
            test_manager,
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
        if assert_expect {
            FUZZ_ADDRESS_ORCHESTRATOR_EXPECT
                .assert_eq(&orchestrator_contract_address.to_hex_string());
            FUZZ_ADDRESS_CAIRO1_A_EXPECT.assert_eq(&cairo1_contract_address_a.to_hex_string());
            FUZZ_ADDRESS_CAIRO1_B_EXPECT.assert_eq(&cairo1_contract_address_b.to_hex_string());
            FUZZ_ADDRESS_CAIRO0_A_EXPECT.assert_eq(&cairo0_contract_address_a.to_hex_string());
            FUZZ_ADDRESS_CAIRO0_B_EXPECT.assert_eq(&cairo0_contract_address_b.to_hex_string());
        }
        test_manager
    }

    pub fn add_random_operation(&mut self, filter: Option<OperationFilter>) -> Result<(), ()> {
        self.context.add_random_operation(filter)
    }

    /// Exhaustively generate all possible operation lists of exactly the given length.
    /// WARNING: The number of lists is exponential in the length! Use with small values only.
    /// A value of 4 can generate over 100K lists!
    pub async fn get_all_operation_lists(
        max_length: usize,
        filter: Option<OperationFilter>,
    ) -> Vec<Vec<FuzzOperationData>> {
        let initial_state = Self::init(0).await;
        initial_state.context.get_all_operation_tails(max_length, filter)
    }

    #[allow(unused)]
    pub fn prettify_operations(&self) -> String {
        self.context.prettify_operations()
    }

    /// Run the fuzz test. Should be called after the operations list is final (no need to finalize
    /// the context - if the finalized state is Ongoing it will be converted to Succeeded).
    pub async fn run_test(mut self) {
        if !self.context.finalized() {
            self.context.final_state = FinalizedState::Succeeded;
        }

        // Initialize the orchestrator contract with the scenario data.
        let scenario_data = self.context.operations_to_scenario_data();
        let orchestrator_calldata = create_calldata(
            *FUZZ_ADDRESS_ORCHESTRATOR,
            "initialize",
            &[vec![Felt::from(scenario_data.len())], scenario_data].concat(),
        );
        self.test_manager
            .add_funded_account_invoke(invoke_tx_args! { calldata: orchestrator_calldata });

        // Invoke the test.
        let start_test_calldata = create_calldata(
            *FUZZ_ADDRESS_ORCHESTRATOR,
            "start_test",
            &[**self.first_called_address],
        );

        // Whether or not a revert is expected depends on context.
        let tx_revert_error = match self.context.final_state {
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

        // Apply expected messages to L1.
        self.test_manager.messages_to_l1 = self.context.expected_messages_to_l1();

        // Run the test.
        let test_output = self.test_manager.build_and_run().await;
        test_output.perform_default_validations();
    }
}

async fn fuzz_test_body(seed: u64, max_n_operations: usize, filter: Option<OperationFilter>) {
    let mut fuzz_tester = FuzzTestManager::init(seed).await;

    // Create scenarios.
    for _ in 0..max_n_operations {
        // An error value means the context is finalized - no more operations can be applied.
        if fuzz_tester.add_random_operation(filter).is_err() {
            break;
        }
    }

    println!("Seed: {seed}.");
    #[cfg(feature = "fuzz_test_debug")]
    println!("{}", fuzz_tester.prettify_operations());

    fuzz_tester.run_test().await;
}

/// Updates the expected fuzz contract addresses (if UPDATE_EXPECT env var is set).
#[rstest]
#[tokio::test]
async fn test_fuzz_deployment_expect() {
    FuzzTestManager::init_deployment(true).await;
}

#[rstest]
#[case::all(None)]
#[case::call_write_panic_return(Some(OP_FILTER_CALL_WRITE_PANIC_RETURN))]
#[tokio::test]
async fn test_daily_fuzz_seed(
    #[case] filter: Option<OperationFilter>,
    #[values(0, 1, 2, 3, 4, 5, 6, 7)] inner_seed: u64,
) {
    let now = Utc::now();
    let day: u64 = now.day().into();
    let month: u64 = now.month().into();
    let year: u64 = now.year().try_into().unwrap();
    let seed = day * 100000000 + month * 1000000 + year * 100 + inner_seed;
    fuzz_test_body(seed, 10, filter).await;
}

#[cfg(feature = "long_fuzz_test")]
mod long_fuzz_test {
    use super::*;

    /// Long fuzz test. This generates a lot of code, so instead of `#[cfg_attr(.., ignore)]`, we
    /// gate the actual module.
    /// It is strongly recommended to run this test in release mode only.
    #[rstest]
    #[tokio::test]
    async fn test_fuzz(
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
        fuzz_test_body(seed, 10, None).await;
    }
}

/// Exhaustive test for all scenarios of length `MAX_EXHAUSTIVE_FUZZ_LENGTH` (or less, if state is
/// finalized before reaching the length).
/// It is strongly recommended to run this test in release mode only.
#[rstest]
#[tokio::test(flavor = "multi_thread")]
#[cfg_attr(
    not(feature = "exhaustive_fuzz_test"),
    ignore = "Skipped - set `exhaustive_fuzz_test` feature to run."
)]
async fn test_exhaustive_fuzz() {
    let operation_lists =
        FuzzTestManager::get_all_operation_lists(MAX_EXHAUSTIVE_FUZZ_LENGTH, None).await;
    let total_scenarios = operation_lists.len();

    macro_rules! push_test_or_break {
        ($tasks_set:ident, $operations_iter:ident, $total_scenarios:ident) => {
            let (index, operations) = match $operations_iter.next() {
                Some(item) => item,
                None => break,
            };
            $tasks_set.spawn(async move {
                println!("Running test {index}/{}.", $total_scenarios);
                FuzzTestManager::init_explicit_test(operations).await.run_test().await;
            });
        };
    }

    // Add `NUM_EXHAUSTIVE_PARALLEL_FUZZ_TESTS` initial tasks.
    let mut operations_iter = operation_lists.into_iter().enumerate();
    let mut tasks_set = JoinSet::new();
    for _ in 0..NUM_EXHAUSTIVE_PARALLEL_FUZZ_TESTS {
        push_test_or_break!(tasks_set, operations_iter, total_scenarios);
    }

    // Wait for a task to complete and add a new task.
    while !tasks_set.is_empty() {
        match tasks_set.join_next().await.unwrap() {
            Err(error) => {
                // A test failed - join all remaining tasks and unwrap the error.
                tasks_set.join_all().await;
                panic!("Fuzz test failed: {error:?}");
            }
            Ok(_) => {
                // A test completed - add a new task for the next operation list.
                push_test_or_break!(tasks_set, operations_iter, total_scenarios);
            }
        }
    }

    // Collect all remaining tasks.
    tasks_set.join_all().await;
}
