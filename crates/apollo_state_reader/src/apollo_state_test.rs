use core::panic;
use std::sync::Arc;
use std::time::{Duration, Instant};

use apollo_class_manager_types::{
    Class,
    ClassHashes,
    ClassId,
    ClassManagerClient,
    ClassManagerClientResult,
    ExecutableClass,
    ExecutableClassHash,
};
use apollo_storage::class::ClassStorageWriter;
use apollo_storage::state::StateStorageWriter;
use apollo_storage::test_utils::get_test_storage;
use apollo_storage::StorageResult;
use assert_matches::assert_matches;
use async_trait::async_trait;
use blockifier::execution::call_info::CallExecution;
use blockifier::execution::entry_point::CallEntryPoint;
use blockifier::retdata;
use blockifier::state::cached_state::CachedState;
use blockifier::state::errors::StateError;
use blockifier::state::state_api::StateReader;
use blockifier::test_utils::contracts::FeatureContractTrait;
use blockifier::test_utils::trivial_external_entry_point_new;
use blockifier_test_utils::cairo_versions::CairoVersion;
use blockifier_test_utils::contracts::FeatureContract;
use indexmap::IndexMap;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::block::BlockNumber;
use starknet_api::contract_class::ContractClass;
use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedClass;
use starknet_api::state::{StateDiff, StorageKey, ThinStateDiff};
use starknet_api::{calldata, felt};

use crate::apollo_state::{ApolloReader, ClassReader};

#[test]
fn test_entry_point_with_papyrus_state() -> StorageResult<()> {
    let ((storage_reader, mut storage_writer), _) = get_test_storage();

    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo0);
    let test_class_hash = test_contract.get_class_hash();
    let test_class = assert_matches!(
        test_contract.get_class(), ContractClass::V0(contract_class) => contract_class
    );

    // Initialize Storage: add test contract and class.
    let deployed_contracts =
        IndexMap::from([(test_contract.get_instance_address(0), test_class_hash)]);
    let state_diff = StateDiff {
        deployed_contracts,
        deprecated_declared_classes: IndexMap::from([(test_class_hash, test_class.clone())]),
        ..Default::default()
    };

    let block_number = BlockNumber::default();
    storage_writer
        .begin_rw_txn()?
        .append_state_diff(block_number, state_diff.into())?
        .append_classes(block_number, Default::default(), &[(test_class_hash, &test_class)])?
        .commit()?;

    // BlockNumber is 1 due to the initialization step above.
    let block_number = BlockNumber(1);
    let apollo_reader = ApolloReader::new(storage_reader, block_number);
    let mut state = CachedState::from(apollo_reader);

    // Call entrypoint that want to write to storage, which updates the cached state's write cache.
    let key = felt!(1234_u16);
    let value = felt!(18_u8);
    let calldata = calldata![key, value];
    let entry_point_call = CallEntryPoint {
        calldata,
        entry_point_selector: selector_from_name("test_storage_read_write"),
        ..trivial_external_entry_point_new(test_contract)
    };
    let storage_address = entry_point_call.storage_address;
    assert_eq!(
        entry_point_call.execute_directly(&mut state).unwrap().execution,
        CallExecution::from_retdata(retdata![value])
    );

    // Verify that the state has changed.
    let storage_key = StorageKey::try_from(key).unwrap();
    let value_from_state = state.get_storage_at(storage_address, storage_key).unwrap();
    assert_eq!(value_from_state, value);

    Ok(())
}

/// Stands in for a class manager that never answers: a network partition, an overloaded remote
/// class manager, or (for a locally-deployed class manager reached over an in-process channel,
/// which carries no request timeout of its own) a stalled component.
struct StalledClassManagerClient;

#[async_trait]
impl ClassManagerClient for StalledClassManagerClient {
    async fn add_class(&self, _class: Class) -> ClassManagerClientResult<ClassHashes> {
        unimplemented!("Not exercised by this test.")
    }

    async fn get_executable(
        &self,
        _class_id: ClassId,
    ) -> ClassManagerClientResult<Option<ExecutableClass>> {
        std::future::pending().await
    }

    async fn get_sierra(&self, _class_id: ClassId) -> ClassManagerClientResult<Option<Class>> {
        unimplemented!("Not exercised by this test.")
    }

    async fn get_executable_class_hash_v2(
        &self,
        _class_id: ClassId,
    ) -> ClassManagerClientResult<Option<ExecutableClassHash>> {
        unimplemented!("Not exercised by this test.")
    }

    async fn add_deprecated_class(
        &self,
        _class_id: ClassId,
        _class: DeprecatedClass,
    ) -> ClassManagerClientResult<()> {
        unimplemented!("Not exercised by this test.")
    }

    async fn add_class_and_executable_unsafe(
        &self,
        _class_id: ClassId,
        _class: Class,
        _executable_class_hash_v2: ExecutableClassHash,
        _executable_class: ExecutableClass,
    ) -> ClassManagerClientResult<()> {
        unimplemented!("Not exercised by this test.")
    }
}

/// Without a `deadline`, a class manager that never answers hangs `get_compiled_class` forever:
/// `ClassReader` blocks the thread it runs on (a shared blocking-pool thread when reached through
/// `call_contract` or block production) inside `block_on`, and nothing can cancel a thread parked
/// there. A `deadline` bounds the wait, so the thread is released within that window instead of
/// being pinned indefinitely.
#[tokio::test(flavor = "multi_thread")]
async fn class_reader_times_out_when_the_class_manager_never_answers() {
    let class_hash = ClassHash(felt!(0x1234_u16));
    // Cairo 1 declaration marker only, with no definition behind it: `is_declared` reads this
    // table, so reading the class must go through the class manager.
    let state_diff = ThinStateDiff {
        class_hash_to_compiled_class_hash: IndexMap::from([(
            class_hash,
            CompiledClassHash::default(),
        )]),
        ..Default::default()
    };

    let ((storage_reader, mut storage_writer), _temp_dir) = get_test_storage();
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber::default(), state_diff)
        .unwrap()
        .commit()
        .unwrap();

    let request_timeout = Duration::from_millis(200);
    let class_reader = Some(ClassReader {
        reader: Arc::new(StalledClassManagerClient),
        runtime: tokio::runtime::Handle::current(),
        deadline: Some(Instant::now() + request_timeout),
    });
    let apollo_reader =
        ApolloReader::new_with_class_reader(storage_reader, BlockNumber(1), class_reader);

    // Bounds the test itself: without the fix, the call below hangs forever, turning a
    // regression into a stuck test rather than a failing one.
    let result = tokio::time::timeout(
        request_timeout * 10,
        tokio::task::spawn_blocking(move || apollo_reader.get_compiled_class(class_hash)),
    )
    .await
    .expect("get_compiled_class did not return within 10x its own request timeout.")
    .expect("Reading a declared class panicked.");

    assert_matches!(
        result,
        Err(StateError::StateReadError(message)) if message.contains("timed out")
    );
}

/// Stands in for a class manager whose executable read succeeds after `executable_delay`, but
/// whose Sierra read never answers.
struct DelayedExecutableThenStalledClassManagerClient {
    executable_delay: Duration,
}

#[async_trait]
impl ClassManagerClient for DelayedExecutableThenStalledClassManagerClient {
    async fn add_class(&self, _class: Class) -> ClassManagerClientResult<ClassHashes> {
        unimplemented!("Not exercised by this test.")
    }

    async fn get_executable(
        &self,
        _class_id: ClassId,
    ) -> ClassManagerClientResult<Option<ExecutableClass>> {
        tokio::time::sleep(self.executable_delay).await;
        Ok(Some(ContractClass::test_casm_contract_class()))
    }

    async fn get_sierra(&self, _class_id: ClassId) -> ClassManagerClientResult<Option<Class>> {
        std::future::pending().await
    }

    async fn get_executable_class_hash_v2(
        &self,
        _class_id: ClassId,
    ) -> ClassManagerClientResult<Option<ExecutableClassHash>> {
        unimplemented!("Not exercised by this test.")
    }

    async fn add_deprecated_class(
        &self,
        _class_id: ClassId,
        _class: DeprecatedClass,
    ) -> ClassManagerClientResult<()> {
        unimplemented!("Not exercised by this test.")
    }

    async fn add_class_and_executable_unsafe(
        &self,
        _class_id: ClassId,
        _class: Class,
        _executable_class_hash_v2: ExecutableClassHash,
        _executable_class: ExecutableClass,
    ) -> ClassManagerClientResult<()> {
        unimplemented!("Not exercised by this test.")
    }
}

/// A single `deadline` must bound `ClassReader`'s total wait, not grant a fresh window to every
/// request: reading a Cairo 1 class needs both `read_casm` and `read_sierra` (mirroring
/// `ApolloReader::read_casm_and_sierra`), and a per-request timeout would let their durations add
/// up past the caller's intended bound instead.
#[tokio::test(flavor = "multi_thread")]
async fn class_reader_deadline_bounds_the_total_wait_across_requests() {
    let class_hash = ClassHash(felt!(0x1234_u16));
    let executable_delay = Duration::from_millis(200);
    let deadline_budget = Duration::from_millis(300);
    let class_reader = ClassReader {
        reader: Arc::new(DelayedExecutableThenStalledClassManagerClient { executable_delay }),
        runtime: tokio::runtime::Handle::current(),
        deadline: Some(Instant::now() + deadline_budget),
    };

    let start = Instant::now();
    let result = tokio::task::spawn_blocking(move || {
        class_reader.read_casm(class_hash)?;
        class_reader.read_sierra(class_hash)
    })
    .await
    .expect("Reading a class panicked.");
    let elapsed = start.elapsed();

    assert_matches!(result, Err(StateError::StateReadError(_)));
    // A per-request (rather than cumulative) timeout would let `read_sierra` start a fresh
    // `deadline_budget` window of its own, pushing this past `executable_delay + deadline_budget`.
    assert!(
        elapsed < executable_delay + deadline_budget,
        "read_sierra was not bounded by the deadline read_casm already spent part of: took \
         {elapsed:?}."
    );
}
