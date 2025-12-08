use core::panic;
use std::sync::Arc;
use std::time::Duration;

use apollo_class_manager_types::{ClassHashes, ClassManagerClient, MockClassManagerClient};
use apollo_starknet_client::reader::PendingData;
use apollo_storage::base_layer::BaseLayerStorageReader;
use apollo_storage::header::HeaderStorageReader;
use apollo_storage::state::StateStorageReader;
use apollo_storage::test_utils::get_test_storage;
use apollo_storage::{StorageError, StorageReader, StorageWriter};
use apollo_test_utils::{get_rng, GetTestInstance};
use assert_matches::assert_matches;
use async_stream::stream;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use futures::StreamExt;
use indexmap::IndexMap;
use papyrus_common::pending_classes::PendingClasses;
use starknet_api::block::{
    Block,
    BlockBody,
    BlockHash,
    BlockHashAndNumber,
    BlockHeader,
    BlockHeaderWithoutHash,
    BlockNumber,
    BlockSignature,
};
use starknet_api::core::{ClassHash, CompiledClassHash, EntryPointSelector, SequencerPublicKey};
use starknet_api::crypto::utils::PublicKey;
use starknet_api::deprecated_contract_class::{ContractClass, EntryPointOffset, EntryPointV0};
use starknet_api::felt;
use starknet_api::state::{SierraContractClass, StateDiff, StateNumber};
use tokio::sync::{Mutex, RwLock};
use tokio::time::sleep;
use tracing::{debug, error};

use super::pending::MockPendingSourceTrait;
use crate::sources::base_layer::{BaseLayerSourceTrait, MockBaseLayerSourceTrait};
use crate::sources::central::{
    BlocksStream,
    CompiledClassesStream,
    MockCentralSourceTrait,
    StateUpdatesStream,
};
use crate::{
    CentralError,
    CentralSourceTrait,
    GenericStateSync,
    StateSyncError,
    StateSyncResult,
    SyncConfig,
};

const SYNC_SLEEP_DURATION: Duration = Duration::from_millis(100); // 100ms
const BASE_LAYER_SLEEP_DURATION: Duration = Duration::from_millis(10); // 10ms
const DURATION_BEFORE_CHECKING_STORAGE: Duration = SYNC_SLEEP_DURATION.saturating_mul(2); // 200ms twice the sleep duration of the sync loop.
const MAX_CHECK_STORAGE_ITERATIONS: u8 = 5;
const STREAM_SIZE: u32 = 1000;

// TODO(dvir): separate this file to flow tests and unit tests.
// TODO(dvir): consider adding a test for mismatch between the base layer and l2.

enum CheckStoragePredicateResult {
    InProgress,
    Passed,
    Error,
}

// Checks periodically if the storage reached a certain state defined by f.
async fn check_storage(
    reader: StorageReader,
    timeout: Duration,
    predicate: impl Fn(&StorageReader) -> CheckStoragePredicateResult,
) -> bool {
    // Let the other thread opportunity to run before starting the check.
    tokio::time::sleep(DURATION_BEFORE_CHECKING_STORAGE).await;
    let interval_time = timeout.div_f32(MAX_CHECK_STORAGE_ITERATIONS.into());
    let mut interval = tokio::time::interval(interval_time);
    for i in 0..MAX_CHECK_STORAGE_ITERATIONS {
        debug!("== Checking predicate on storage ({}/{}). ==", i + 1, MAX_CHECK_STORAGE_ITERATIONS);
        match predicate(&reader) {
            CheckStoragePredicateResult::InProgress => {
                debug!("== Check finished, test still in progress. ==");
                interval.tick().await;
            }
            CheckStoragePredicateResult::Passed => {
                debug!("== Check passed. ==");
                return true;
            }
            CheckStoragePredicateResult::Error => {
                debug!("== Check failed. ==");
                return false;
            }
        }
    }
    error!("Check storage timed out.");
    false
}

fn get_test_sync_config(verify_blocks: bool) -> SyncConfig {
    SyncConfig {
        block_propagation_sleep_duration: SYNC_SLEEP_DURATION,
        base_layer_propagation_sleep_duration: BASE_LAYER_SLEEP_DURATION,
        recoverable_error_sleep_duration: SYNC_SLEEP_DURATION,
        blocks_max_stream_size: STREAM_SIZE,
        state_updates_max_stream_size: STREAM_SIZE,
        verify_blocks,
        collect_pending_data: false,
        // TODO(Shahak): Add test where store_sierras_and_casms is set to false.
        store_sierras_and_casms: true,
    }
}

// Runs sync loop with a mocked central - infinite loop unless panicking.
async fn run_sync(
    reader: StorageReader,
    writer: StorageWriter,
    central: impl CentralSourceTrait + Send + Sync + 'static,
    base_layer: impl BaseLayerSourceTrait + Send + Sync,
    config: SyncConfig,
    class_manager_client: Option<Arc<dyn ClassManagerClient>>,
) -> StateSyncResult {
    // Mock to the pending source that always returns the default pending data.
    let mut pending_source = MockPendingSourceTrait::new();
    pending_source.expect_get_pending_data().returning(|| Ok(PendingData::default()));

    let state_sync = GenericStateSync {
        config,
        shared_highest_block: Arc::new(RwLock::new(None)),
        pending_data: Arc::new(RwLock::new(PendingData::default())),
        central_source: Arc::new(central),
        pending_source: Arc::new(pending_source),
        pending_classes: Arc::new(RwLock::new(PendingClasses::default())),
        base_layer_source: Some(Arc::new(Mutex::new(base_layer))),
        reader,
        writer: Arc::new(Mutex::new(writer)),
        sequencer_pub_key: None,
        // TODO(shahak): Add test with mock class manager client.
        // TODO(shahak): Add test with post 0.14.0 block and mock class manager client and see that
        // up until that block we call add_class_and_executable_unsafe and from that block we call
        // add_class.
        class_manager_client,
    };

    state_sync.run().await?;
    Ok(())
}

#[tokio::test]
async fn sync_empty_chain() {
    let _ = simple_logger::init_with_env();

    // Mock central without any block.
    let mut central_mock = MockCentralSourceTrait::new();
    central_mock.expect_get_latest_block().returning(|| Ok(None));

    // Mock base_layer without any block.
    let mut base_layer_mock = MockBaseLayerSourceTrait::new();
    base_layer_mock.expect_latest_proved_block().returning(|| Ok(None));

    let ((reader, writer), _temp_dir) = get_test_storage();
    let class_manager_client = None;
    let sync_future = run_sync(
        reader.clone(),
        writer,
        central_mock,
        base_layer_mock,
        get_test_sync_config(false),
        class_manager_client,
    );

    // Check that the header marker is 0.
    let check_storage_future = check_storage(reader.clone(), Duration::from_millis(50), |reader| {
        let marker = reader.begin_ro_txn().unwrap().get_header_marker().unwrap();
        if marker == BlockNumber(0) {
            return CheckStoragePredicateResult::Passed;
        }
        CheckStoragePredicateResult::Error
    });

    tokio::select! {
        sync_result = sync_future => sync_result.unwrap(),
        storage_check_result = check_storage_future => assert!(storage_check_result),
    }
}

#[tokio::test]
async fn sync_happy_flow() {
    const N_BLOCKS: u64 = 5;
    const LATEST_BLOCK_NUMBER: BlockNumber = BlockNumber(N_BLOCKS - 1);
    const DEPLOY_BLOCK_NUMBER: BlockNumber = BlockNumber(N_BLOCKS - 2);
    // FIXME: (Omri) analyze and set a lower value.
    const MAX_TIME_TO_SYNC_MS: u64 = 800;
    let _ = simple_logger::init_with_env();

    let class_hash_1 = ClassHash(felt!("0x1"));
    let compiled_class_hash_1 = CompiledClassHash(felt!("0x101"));
    let class_hash_2 = ClassHash(felt!("0x2"));
    let compiled_class_hash_2 = CompiledClassHash(felt!("0x102"));
    let deployed_class_hash = ClassHash(felt!("0x3"));
    let deprecated_class_hash = ClassHash(felt!("0x4"));

    let mut rng = get_rng();
    let mut deployed_class = ContractClass::get_test_instance(&mut rng);
    deployed_class.entry_points_by_type.insert(
        Default::default(),
        vec![EntryPointV0 {
            selector: EntryPointSelector::default(),
            offset: EntryPointOffset(123),
        }],
    );
    let mut deprecated_class = ContractClass::get_test_instance(&mut rng);
    deprecated_class.entry_points_by_type.insert(
        Default::default(),
        vec![EntryPointV0 {
            selector: EntryPointSelector::default(),
            offset: EntryPointOffset(124),
        }],
    );

    // Mock having N_BLOCKS chain in central.
    let mut central_mock = MockCentralSourceTrait::new();
    central_mock.expect_get_latest_block().returning(|| {
        Ok(Some(BlockHashAndNumber {
            number: LATEST_BLOCK_NUMBER,
            hash: create_block_hash(LATEST_BLOCK_NUMBER, false),
        }))
    });
    central_mock.expect_stream_new_blocks().returning(move |initial, up_to| {
        let blocks_stream: BlocksStream<'_> = stream! {
            for block_number in initial.iter_up_to(up_to) {
                if block_number.0 >= N_BLOCKS {
                    yield Err(CentralError::BlockNotFound { block_number });
                }
                let header = BlockHeader {
                    block_hash: create_block_hash(block_number, false),
                    block_header_without_hash: BlockHeaderWithoutHash {
                        block_number,
                        parent_hash: create_block_hash(block_number.prev().unwrap_or_default(), false),
                        ..Default::default()
                    },
                    ..Default::default()
                };
                yield Ok((
                    block_number,
                    Block { header, body: BlockBody::default() },
                    BlockSignature::default(),
                ));
            }
        }
        .boxed();
        blocks_stream
    });

    let expected_deprecated_class = deprecated_class.clone();
    let expected_deployed_class = deployed_class.clone();
    central_mock.expect_stream_state_updates().returning(move |initial, up_to| {
        let deprecated_class = deprecated_class.clone();
        let deployed_class = deployed_class.clone();
        let state_stream: StateUpdatesStream<'_> = stream! {
            for block_number in initial.iter_up_to(up_to) {
                if block_number.0 >= N_BLOCKS {
                    yield Err(CentralError::BlockNotFound { block_number })
                }

                // Add declared classes to specific blocks to test compiled class hash mapping
                let mut state_diff = match block_number.0 {
                    1 => StateDiff {
                        declared_classes: IndexMap::from([
                            (class_hash_1, (compiled_class_hash_1, SierraContractClass::default())),
                        ]),
                        ..Default::default()
                    },
                    3 => StateDiff {
                        declared_classes: IndexMap::from([
                            (class_hash_2, (compiled_class_hash_2, SierraContractClass::default())),
                        ]),
                        ..Default::default()
                    },
                    _ => StateDiff::default(),
                };
                // For the last block, include test data for deployed class definitions
                let mut deployed_contract_class_definitions = IndexMap::new();
                if block_number.0 >= DEPLOY_BLOCK_NUMBER.0 {
                    deployed_contract_class_definitions = IndexMap::from([(deployed_class_hash, deployed_class.clone())]);
                }

                if block_number == LATEST_BLOCK_NUMBER {
                    state_diff.deprecated_declared_classes = IndexMap::from([(deprecated_class_hash, deprecated_class.clone())]);
                }

                yield Ok((
                    block_number,
                    create_block_hash(block_number, false),
                    state_diff,
                    deployed_contract_class_definitions,
                ));
            }
        }
        .boxed();
        state_stream
    });

    // Add compiled classes stream mock
    central_mock.expect_stream_compiled_classes().returning(move |initial, up_to| {
        let compiled_classes_stream: CompiledClassesStream<'_> = stream! {
            for block_number in initial.iter_up_to(up_to) {
                if block_number.0 >= N_BLOCKS {
                    yield Err(CentralError::BlockNotFound { block_number });
                }

                // Return compiled classes for blocks that declared them
                match block_number.0 {
                    1 => {
                        let mut rng = get_rng();
                        yield Ok((
                            class_hash_1,
                            compiled_class_hash_1,
                            CasmContractClass::get_test_instance(&mut rng),
                        ));
                    },
                    3 => {
                        let mut rng = get_rng();
                        yield Ok((
                            class_hash_2,
                            compiled_class_hash_2,
                            CasmContractClass::get_test_instance(&mut rng),
                        ));
                    },
                    _ => {}
                }
            }
        }
        .boxed();
        compiled_classes_stream
    });

    central_mock.expect_get_block_hash().returning(|bn| Ok(Some(create_block_hash(bn, false))));

    // TODO(dvir): find a better way to do this.
    let mut base_layer_mock = MockBaseLayerSourceTrait::new();
    let mut base_layer_call_counter = 0;
    base_layer_mock.expect_latest_proved_block().returning(move || {
        base_layer_call_counter += 1;
        Ok(match base_layer_call_counter {
            1 => None,
            2 => Some((
                BlockNumber(N_BLOCKS - 2),
                create_block_hash(BlockNumber(N_BLOCKS - 2), false),
            )),
            _ => Some((
                BlockNumber(N_BLOCKS - 1),
                create_block_hash(BlockNumber(N_BLOCKS - 1), false),
            )),
        })
    });

    // Create mock class manager client with expectations
    let mut mock_class_manager = MockClassManagerClient::new();

    // Expect add_deprecated_class to be called for both class hashes
    mock_class_manager.expect_add_class().times(1).returning(move |_class| {
        Ok(ClassHashes { class_hash: class_hash_1, ..Default::default() })
    });
    mock_class_manager.expect_add_class().times(1).returning(move |_class| {
        Ok(ClassHashes { class_hash: class_hash_2, ..Default::default() })
    });
    mock_class_manager
        .expect_add_deprecated_class()
        .withf(move |class_hash, _class| *class_hash == deployed_class_hash)
        .times(1)
        .returning(|_class_hash, _class| Ok(()));
    mock_class_manager
        .expect_add_deprecated_class()
        .withf(move |class_hash, _class| *class_hash == deprecated_class_hash)
        .times(1)
        .returning(|_class_hash, _class| Ok(()));

    let ((reader, writer), _temp_dir) = get_test_storage();
    let sync_future = run_sync(
        reader.clone(),
        writer,
        central_mock,
        base_layer_mock,
        get_test_sync_config(false),
        Some(Arc::new(mock_class_manager)),
    );

    // Check that the storage reached N_BLOCKS within MAX_TIME_TO_SYNC_MS.
    let check_storage_future =
        check_storage(reader, Duration::from_millis(MAX_TIME_TO_SYNC_MS), |reader| {
            let header_marker = reader.begin_ro_txn().unwrap().get_header_marker().unwrap();
            debug!("Header marker currently at {}", header_marker);
            if header_marker < BlockNumber(N_BLOCKS) {
                return CheckStoragePredicateResult::InProgress;
            }
            if header_marker > BlockNumber(N_BLOCKS) {
                return CheckStoragePredicateResult::Error;
            }

            let state_marker = reader.begin_ro_txn().unwrap().get_state_marker().unwrap();
            debug!("State marker currently at {}", state_marker);
            if state_marker < BlockNumber(N_BLOCKS) {
                return CheckStoragePredicateResult::InProgress;
            }
            if state_marker > BlockNumber(N_BLOCKS) {
                return CheckStoragePredicateResult::Error;
            }

            let base_layer_marker =
                reader.begin_ro_txn().unwrap().get_base_layer_block_marker().unwrap();
            debug!("Base layer marker currently at {base_layer_marker}");
            if base_layer_marker < BlockNumber(N_BLOCKS) {
                return CheckStoragePredicateResult::InProgress;
            }
            if base_layer_marker > BlockNumber(N_BLOCKS) {
                return CheckStoragePredicateResult::Error;
            }

            // Verify compiled class hash mappings are stored correctly
            let txn = reader.begin_ro_txn().unwrap();
            let state_reader = txn.get_state_reader().unwrap();

            // Check mappings - return InProgress if not ready, otherwise assert equality
            let state_number_1 = StateNumber::unchecked_right_after_block(BlockNumber(1));
            let state_number_2 = StateNumber::unchecked_right_after_block(BlockNumber(3));
            // Arbitrary state number after the last block.
            let state_number_final = StateNumber::unchecked_right_after_block(BlockNumber(4));

            let hash_1 =
                state_reader.get_compiled_class_hash_at(state_number_1, &class_hash_1).unwrap();
            let hash_2 =
                state_reader.get_compiled_class_hash_at(state_number_2, &class_hash_2).unwrap();
            let hash_final =
                state_reader.get_compiled_class_hash_at(state_number_final, &class_hash_1).unwrap();

            if hash_1.is_none() || hash_2.is_none() || hash_final.is_none() {
                return CheckStoragePredicateResult::InProgress;
            }

            assert_eq!(hash_1, Some(compiled_class_hash_1));
            assert_eq!(hash_2, Some(compiled_class_hash_2));
            // Ensure class hash 1 remains unchanged after later declarations.
            assert_eq!(hash_final, Some(compiled_class_hash_1));

            // Verify that the deprecated class definition block number is the same as the first
            // block that the class was declared in.
            let deploy_block_number = state_reader
                .get_deprecated_class_definition_block_number(&deployed_class_hash)
                .unwrap();
            assert_eq!(deploy_block_number, Some(DEPLOY_BLOCK_NUMBER));
            let declare_deprecated_block_number = state_reader
                .get_deprecated_class_definition_block_number(&deprecated_class_hash)
                .unwrap();
            assert_eq!(declare_deprecated_block_number, Some(LATEST_BLOCK_NUMBER));

            // Verify that the deprecated contract class is the same as the one that was declared.
            let deploy_class = state_reader
                .get_deprecated_class_definition_at(
                    StateNumber::unchecked_right_after_block(LATEST_BLOCK_NUMBER),
                    &deployed_class_hash,
                )
                .unwrap();
            assert_eq!(deploy_class, Some(expected_deployed_class.clone()));

            let declare_deprecated_class = state_reader
                .get_deprecated_class_definition_at(
                    StateNumber::unchecked_right_after_block(LATEST_BLOCK_NUMBER),
                    &deprecated_class_hash,
                )
                .unwrap();
            assert_eq!(declare_deprecated_class, Some(expected_deprecated_class.clone()));

            CheckStoragePredicateResult::Passed
        });

    tokio::select! {
        _ = sleep(Duration::from_secs(1)) => panic!("Test timed out."),
        sync_result = sync_future => sync_result.unwrap(),
        storage_check_result = check_storage_future => assert!(storage_check_result),
    }
}

#[tokio::test]
async fn test_unrecoverable_sync_error_flow() {
    let _ = simple_logger::init_with_env();

    const LATEST_BLOCK_NUMBER: BlockNumber = BlockNumber(0);
    const BLOCK_NUMBER: BlockNumber = BlockNumber(1);
    const WRONG_BLOCK_NUMBER: BlockNumber = BlockNumber(2);

    // Mock central with one block but return wrong header.
    let mut mock = MockCentralSourceTrait::new();
    mock.expect_get_latest_block().returning(|| {
        Ok(Some(BlockHashAndNumber {
            number: LATEST_BLOCK_NUMBER,
            hash: create_block_hash(LATEST_BLOCK_NUMBER, false),
        }))
    });
    mock.expect_stream_new_blocks().returning(move |_, _| {
        let blocks_stream: BlocksStream<'_> = stream! {
            let header = BlockHeader {
                    block_hash: create_block_hash(BLOCK_NUMBER, false),
                    block_header_without_hash: BlockHeaderWithoutHash {
                        block_number: BLOCK_NUMBER,
                        parent_hash: create_block_hash(BLOCK_NUMBER.prev().unwrap_or_default(), false),
                        ..Default::default()
                    },
                    ..Default::default()
                };
            yield Ok((
                BLOCK_NUMBER,
                Block { header, body: BlockBody::default()},
                BlockSignature::default(),
            ));
        }
        .boxed();
        blocks_stream
    });
    mock.expect_stream_state_updates().returning(move |_, _| {
        let state_stream: StateUpdatesStream<'_> = stream! {
            yield Ok((
                BLOCK_NUMBER,
                create_block_hash(BLOCK_NUMBER, false),
                StateDiff::default(),
                IndexMap::new(),
            ));
        }
        .boxed();
        state_stream
    });
    // make get_block_hash return a hash for the wrong block number
    mock.expect_get_block_hash()
        .returning(|_| Ok(Some(create_block_hash(WRONG_BLOCK_NUMBER, false))));

    let ((reader, writer), _temp_dir) = get_test_storage();
    let class_manager_client = None;
    let sync_future = run_sync(
        reader.clone(),
        writer,
        mock,
        MockBaseLayerSourceTrait::new(),
        get_test_sync_config(false),
        class_manager_client,
    );
    let sync_res = tokio::join! {sync_future};
    assert!(sync_res.0.is_err());
    // expect sync to raise the unrecoverable error it gets. In this case a DB Inconsistency error.
    assert_matches!(
        sync_res.0.unwrap_err(),
        StateSyncError::StorageError(StorageError::DBInconsistency { msg: _ })
    );
}

#[tokio::test]
async fn sequencer_pub_key_management() {
    let _ = simple_logger::init_with_env();

    let first_sequencer_pub_key = SequencerPublicKey(PublicKey(felt!("0x111")));
    let second_sequencer_pub_key = SequencerPublicKey(PublicKey(felt!("0x222")));
    let first_copy = first_sequencer_pub_key;

    let mut central_mock = MockCentralSourceTrait::new();
    // Mock error in sync loop so the public key will be requested again over and over.
    central_mock
        .expect_get_latest_block()
        .returning(|| Err(CentralError::BlockNotFound { block_number: BlockNumber(0) }));

    // Mock sequencer pub key change after the second request.
    central_mock.expect_get_sequencer_pub_key().times(2).returning(move || Ok(first_copy));
    central_mock.expect_get_sequencer_pub_key().returning(move || Ok(second_sequencer_pub_key));

    // Mock base_layer without any block.
    let mut base_layer_mock = MockBaseLayerSourceTrait::new();
    base_layer_mock.expect_latest_proved_block().returning(|| Ok(None));

    let ((reader, writer), _temp_dir) = get_test_storage();
    let config = get_test_sync_config(true);
    let class_manager_client = None;
    let sync_future = run_sync(
        reader.clone(),
        writer,
        central_mock,
        base_layer_mock,
        config,
        class_manager_client,
    );

    let sync_result =
        tokio::time::timeout(config.block_propagation_sleep_duration * 4, sync_future)
            .await
            .unwrap()
            .expect_err("Expecting sync to fail due to sequencer pub key change.");

    assert_matches!(
        sync_result,
        StateSyncError::SequencerPubKeyChanged { old, new }
            if old == first_copy && new == second_sequencer_pub_key
    );
}

fn create_block_hash(bn: BlockNumber, is_reverted_block: bool) -> BlockHash {
    if is_reverted_block {
        BlockHash(felt!(format!("0x{}10", bn.0).as_str()))
    } else {
        BlockHash(felt!(format!("0x{}", bn.0).as_str()))
    }
}
