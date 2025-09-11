use std::fmt::Debug;
use std::sync::Arc;

use apollo_class_manager_types::{MockClassManagerClient, SharedClassManagerClient};
use apollo_network::network_manager::test_utils::{
    create_test_server_query_manager,
    mock_register_sqmr_protocol_server,
};
use apollo_network::network_manager::ServerQueryManager;
use apollo_protobuf::converters::ProtobufConversionError;
use apollo_protobuf::sync::{
    BlockHashOrNumber,
    ClassQuery,
    DataOrFin,
    Direction,
    EventQuery,
    HeaderQuery,
    Query,
    SignedBlockHeader,
    StateDiffChunk,
    StateDiffQuery,
    TransactionQuery,
};
use apollo_storage::body::BodyStorageWriter;
use apollo_storage::class_manager::ClassManagerStorageWriter;
use apollo_storage::header::{HeaderStorageReader, HeaderStorageWriter};
use apollo_storage::state::StateStorageWriter;
use apollo_storage::test_utils::get_test_storage;
use apollo_storage::{StorageReader, StorageWriter};
use apollo_test_utils::{get_rng, get_test_body, GetTestInstance};
use futures::channel::mpsc::Sender;
use futures::StreamExt;
use lazy_static::lazy_static;
use mockall::predicate::eq;
use papyrus_common::pending_classes::ApiContractClass;
use papyrus_common::state::create_random_state_diff;
use rand::random;
use starknet_api::block::{
    BlockBody,
    BlockHash,
    BlockHeader,
    BlockHeaderWithoutHash,
    BlockNumber,
    BlockSignature,
};
use starknet_api::contract_class::ContractClass;
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::state::SierraContractClass;
use starknet_api::transaction::{
    Event,
    FullTransaction,
    Transaction,
    TransactionHash,
    TransactionOutput,
};

use super::{split_thin_state_diff, FetchBlockData, P2pSyncServer, P2pSyncServerChannels};
use crate::server::register_query;
const BUFFER_SIZE: usize = 10;
const NUM_OF_BLOCKS: usize = 10;
const NUM_TXS_PER_BLOCK: usize = 5;
const EVENTS_PER_TX: usize = 2;
const BLOCKS_DELTA: usize = 5;

enum StartBlockType {
    Hash,
    Number,
}

// TODO(shahak): Change tests to use channels and not register_query
#[tokio::test]
async fn header_query_positive_flow() {
    let assert_signed_block_header = |data: Vec<SignedBlockHeader>| {
        let len = data.len();
        assert_eq!(len, NUM_OF_BLOCKS);
        for (i, signed_header) in data.into_iter().enumerate() {
            assert_eq!(
                signed_header.block_header.block_header_without_hash.block_number.0,
                u64::try_from(i).unwrap()
            );
        }
    };

    run_test::<_, _, HeaderQuery>(assert_signed_block_header, 0, StartBlockType::Hash).await;
    run_test::<_, _, HeaderQuery>(assert_signed_block_header, 0, StartBlockType::Number).await;
}

#[tokio::test]
async fn transaction_query_positive_flow() {
    let assert_transaction_and_output = |data: Vec<FullTransaction>| {
        let len = data.len();
        assert_eq!(len, NUM_OF_BLOCKS * NUM_TXS_PER_BLOCK);
        for (i, FullTransaction { transaction, transaction_output, transaction_hash }) in
            data.into_iter().enumerate()
        {
            assert_eq!(transaction, TXS[i / NUM_TXS_PER_BLOCK][i % NUM_TXS_PER_BLOCK]);
            assert_eq!(
                transaction_output,
                TX_OUTPUTS[i / NUM_TXS_PER_BLOCK][i % NUM_TXS_PER_BLOCK]
            );
            assert_eq!(transaction_hash, TX_HASHES[i / NUM_TXS_PER_BLOCK][i % NUM_TXS_PER_BLOCK]);
        }
    };

    run_test::<_, _, TransactionQuery>(assert_transaction_and_output, 0, StartBlockType::Hash)
        .await;
    run_test::<_, _, TransactionQuery>(assert_transaction_and_output, 0, StartBlockType::Number)
        .await;
}

#[tokio::test]
async fn state_diff_query_positive_flow() {
    let assert_state_diff_chunk = |data: Vec<StateDiffChunk>| {
        assert_eq!(data.len(), STATE_DIFF_CHUNCKS.len());

        for (data, expected_data) in data.iter().zip(STATE_DIFF_CHUNCKS.iter()) {
            assert_eq!(data, expected_data);
        }
    };
    run_test::<_, _, StateDiffQuery>(assert_state_diff_chunk, 0, StartBlockType::Hash).await;
    run_test::<_, _, StateDiffQuery>(assert_state_diff_chunk, 0, StartBlockType::Number).await;
}

#[tokio::test]
async fn event_query_positive_flow() {
    let assert_event = |data: Vec<(Event, TransactionHash)>| {
        assert_eq!(data.len(), NUM_OF_BLOCKS * NUM_TXS_PER_BLOCK * EVENTS_PER_TX);
        for (i, (event, tx_hash)) in data.into_iter().enumerate() {
            assert_eq!(
                tx_hash,
                TX_HASHES[i / (NUM_TXS_PER_BLOCK * EVENTS_PER_TX)]
                    [i / EVENTS_PER_TX % NUM_TXS_PER_BLOCK]
            );
            assert_eq!(
                event,
                EVENTS[i / (NUM_TXS_PER_BLOCK * EVENTS_PER_TX)
                    + i / EVENTS_PER_TX % NUM_TXS_PER_BLOCK]
            );
        }
    };

    run_test::<_, _, EventQuery>(assert_event, 0, StartBlockType::Hash).await;
    run_test::<_, _, EventQuery>(assert_event, 0, StartBlockType::Number).await;
}

#[tokio::test]
async fn class_query_positive_flow() {
    let assert_class = |data: Vec<(ApiContractClass, ClassHash)>| {
        // create_random_state_diff creates a state diff with 1 declared class
        // and 1 deprecated declared class
        assert_eq!(data.len(), CLASSES_WITH_HASHES.len() + DEPRECATED_CLASSES_WITH_HASHES.len());
        for (i, data) in data.iter().enumerate() {
            match data {
                (ApiContractClass::ContractClass(contract_class), class_hash) => {
                    let (expected_class_hash, expected_contract_class) =
                        &CLASSES_WITH_HASHES[i / 2][0];
                    assert_eq!(contract_class, expected_contract_class);
                    assert_eq!(class_hash, expected_class_hash);
                }
                (
                    ApiContractClass::DeprecatedContractClass(deprecated_contract_class),
                    class_hash,
                ) => {
                    let (expected_class_hash, expected_contract_class) =
                        &DEPRECATED_CLASSES_WITH_HASHES[i / 2][0];
                    assert_eq!(deprecated_contract_class, expected_contract_class);
                    assert_eq!(class_hash, expected_class_hash);
                }
            }
        }
    };
    run_test::<_, _, ClassQuery>(assert_class, 0, StartBlockType::Hash).await;
    run_test::<_, _, ClassQuery>(assert_class, 0, StartBlockType::Number).await;
}

#[tokio::test]
async fn header_query_some_blocks_are_missing() {
    let assert_signed_block_header = |data: Vec<SignedBlockHeader>| {
        let len = data.len();
        assert!(len == BLOCKS_DELTA);
        for (i, signed_header) in data.into_iter().enumerate() {
            assert_eq!(
                signed_header.block_header.block_header_without_hash.block_number.0,
                u64::try_from(i + NUM_OF_BLOCKS - BLOCKS_DELTA).unwrap()
            );
        }
    };

    run_test::<_, _, HeaderQuery>(
        assert_signed_block_header,
        NUM_OF_BLOCKS - BLOCKS_DELTA,
        StartBlockType::Number,
    )
    .await;
}

#[tokio::test]
async fn transaction_query_some_blocks_are_missing() {
    let assert_transaction_and_output = |data: Vec<FullTransaction>| {
        let len = data.len();
        assert!(len == (BLOCKS_DELTA * NUM_TXS_PER_BLOCK));
        for (i, FullTransaction { transaction, transaction_output, transaction_hash }) in
            data.into_iter().enumerate()
        {
            assert_eq!(
                transaction,
                TXS[i / NUM_TXS_PER_BLOCK + NUM_OF_BLOCKS - BLOCKS_DELTA][i % NUM_TXS_PER_BLOCK]
            );
            assert_eq!(
                transaction_output,
                TX_OUTPUTS[i / NUM_TXS_PER_BLOCK + NUM_OF_BLOCKS - BLOCKS_DELTA]
                    [i % NUM_TXS_PER_BLOCK]
            );
            assert_eq!(
                transaction_hash,
                TX_HASHES[i / NUM_TXS_PER_BLOCK + NUM_OF_BLOCKS - BLOCKS_DELTA]
                    [i % NUM_TXS_PER_BLOCK]
            );
        }
    };

    run_test::<_, _, TransactionQuery>(
        assert_transaction_and_output,
        NUM_OF_BLOCKS - BLOCKS_DELTA,
        StartBlockType::Number,
    )
    .await;
}

#[tokio::test]
async fn state_diff_query_some_blocks_are_missing() {
    let assert_state_diff_chunk = |data: Vec<StateDiffChunk>| {
        // create_random_state_diff creates a state diff with 5 chunks.
        const STATE_DIFF_CHUNK_PER_BLOCK: usize = 5;
        assert_eq!(data.len(), BLOCKS_DELTA * STATE_DIFF_CHUNK_PER_BLOCK);
        for (i, data) in data.into_iter().enumerate() {
            assert_eq!(
                data,
                STATE_DIFF_CHUNCKS[i + (NUM_OF_BLOCKS - BLOCKS_DELTA) * STATE_DIFF_CHUNK_PER_BLOCK]
            );
        }
    };

    run_test::<_, _, StateDiffQuery>(
        assert_state_diff_chunk,
        NUM_OF_BLOCKS - BLOCKS_DELTA,
        StartBlockType::Number,
    )
    .await;
}

#[tokio::test]
async fn event_query_some_blocks_are_missing() {
    let assert_event = |data: Vec<(Event, TransactionHash)>| {
        let len = data.len();
        assert_eq!(len, BLOCKS_DELTA * NUM_TXS_PER_BLOCK * EVENTS_PER_TX);
        for (i, (event, tx_hash)) in data.into_iter().enumerate() {
            assert_eq!(
                tx_hash,
                TX_HASHES[i / (NUM_TXS_PER_BLOCK * EVENTS_PER_TX) + (NUM_OF_BLOCKS - BLOCKS_DELTA)]
                    [i / EVENTS_PER_TX % NUM_TXS_PER_BLOCK]
            );
            assert_eq!(
                event,
                EVENTS[i / (NUM_TXS_PER_BLOCK * EVENTS_PER_TX)
                    + (NUM_OF_BLOCKS - BLOCKS_DELTA)
                    + i / EVENTS_PER_TX % NUM_TXS_PER_BLOCK]
            );
        }
    };

    run_test::<_, _, EventQuery>(
        assert_event,
        NUM_OF_BLOCKS - BLOCKS_DELTA,
        StartBlockType::Number,
    )
    .await;
}

#[tokio::test]
async fn class_query_some_blocks_are_missing() {
    let assert_class = |data: Vec<(ApiContractClass, ClassHash)>| {
        // create_random_state_diff creates a state diff with 1 declared class
        // and 1 deprecated declared class
        assert_eq!(data.len(), BLOCKS_DELTA * 2);
        for (i, data) in data.iter().enumerate() {
            match data {
                (ApiContractClass::ContractClass(contract_class), class_hash) => {
                    let (expected_class_hash, expected_contract_class) =
                        &CLASSES_WITH_HASHES[i / 2 + NUM_OF_BLOCKS - BLOCKS_DELTA][0];
                    assert_eq!(contract_class, expected_contract_class);
                    assert_eq!(class_hash, expected_class_hash);
                }
                (
                    ApiContractClass::DeprecatedContractClass(deprecated_contract_class),
                    class_hash,
                ) => {
                    let (expected_class_hash, expected_contract_class) =
                        &DEPRECATED_CLASSES_WITH_HASHES[i / 2 + NUM_OF_BLOCKS - BLOCKS_DELTA][0];
                    assert_eq!(deprecated_contract_class, expected_contract_class);
                    assert_eq!(class_hash, expected_class_hash);
                }
            }
        }
    };
    run_test::<_, _, ClassQuery>(
        assert_class,
        NUM_OF_BLOCKS - BLOCKS_DELTA,
        StartBlockType::Number,
    )
    .await;
}

async fn run_test<T, F, TQuery>(
    assert_fn: F,
    start_block_number: usize,
    start_block_type: StartBlockType,
) where
    T: FetchBlockData + std::fmt::Debug + PartialEq + Send + Sync + 'static,
    F: FnOnce(Vec<T>),
    TQuery: From<Query>
        + TryFrom<Vec<u8>, Error = ProtobufConversionError>
        + Send
        + Debug
        + Clone
        + 'static,
    <TQuery as TryFrom<Vec<u8>>>::Error: Clone,
    Query: From<TQuery>,
{
    let TestArgs {
        p2p_sync_server,
        storage_reader,
        header_sender: _header_sender,
        state_diff_sender: _state_diff_sender,
        transaction_sender: _transaction_sender,
        class_sender: _class_sender,
        event_sender: _event_sender,
    } = setup_sync_server_and_storage();

    let block_number = BlockNumber(start_block_number.try_into().unwrap());
    let start_block = match start_block_type {
        StartBlockType::Hash => BlockHashOrNumber::Hash(
            storage_reader
                .begin_ro_txn()
                .unwrap()
                .get_block_header(block_number)
                .unwrap()
                .unwrap()
                .block_hash,
        ),
        StartBlockType::Number => BlockHashOrNumber::Number(block_number),
    };

    // register a query.
    let query = Query {
        start_block,
        direction: Direction::Forward,
        limit: NUM_OF_BLOCKS.try_into().unwrap(),
        step: 1,
    };
    let query = TQuery::from(query);
    let (server_query_manager, _report_sender, response_reciever) =
        create_test_server_query_manager(query);

    register_query::<T, TQuery>(
        storage_reader,
        server_query_manager,
        p2p_sync_server.class_manager_client.clone(),
        "test",
    );

    // run p2p_sync_server and collect query results.
    tokio::select! {
        _never = p2p_sync_server.run() => {
            unreachable!("Return type Never should never be constructed");
        },
        mut res = response_reciever.collect::<Vec<_>>() => {
            assert_eq!(DataOrFin(None), res.pop().unwrap());
            let filtered_res: Vec<T> = res.into_iter()
                    .map(|data| data.0.expect("P2pSyncServer returned Fin and then returned another response"))
                    .collect();
            assert_fn(filtered_res);
        }
    }
}

pub struct TestArgs {
    #[allow(clippy::type_complexity)]
    pub p2p_sync_server: P2pSyncServer,
    pub storage_reader: StorageReader,
    pub header_sender: Sender<ServerQueryManager<HeaderQuery, DataOrFin<SignedBlockHeader>>>,
    pub state_diff_sender: Sender<ServerQueryManager<StateDiffQuery, DataOrFin<StateDiffChunk>>>,
    pub transaction_sender:
        Sender<ServerQueryManager<TransactionQuery, DataOrFin<FullTransaction>>>,
    pub class_sender:
        Sender<ServerQueryManager<ClassQuery, DataOrFin<(ApiContractClass, ClassHash)>>>,
    pub event_sender: Sender<ServerQueryManager<EventQuery, DataOrFin<(Event, TransactionHash)>>>,
}

#[allow(clippy::type_complexity)]
fn setup_sync_server_and_storage() -> TestArgs {
    let ((storage_reader, mut storage_writer), _temp_dir) = get_test_storage();

    // put some data in the storage.
    insert_to_storage_test_blocks_up_to(&mut storage_writer);

    // put classes into class manager and update marker in storage
    let class_manager_client = create_mock_class_manager_with_blocks_up_to(&mut storage_writer);

    let (header_receiver, header_sender) = mock_register_sqmr_protocol_server(BUFFER_SIZE);
    let (state_diff_receiver, state_diff_sender) = mock_register_sqmr_protocol_server(BUFFER_SIZE);
    let (transaction_receiver, transaction_sender) =
        mock_register_sqmr_protocol_server(BUFFER_SIZE);
    let (class_receiver, class_sender) = mock_register_sqmr_protocol_server(BUFFER_SIZE);
    let (event_receiver, event_sender) = mock_register_sqmr_protocol_server(BUFFER_SIZE);
    let p2p_sync_server_channels = P2pSyncServerChannels {
        header_receiver,
        state_diff_receiver,
        transaction_receiver,
        class_receiver,
        event_receiver,
    };

    let p2p_sync_server = super::P2pSyncServer::new(
        storage_reader.clone(),
        p2p_sync_server_channels,
        class_manager_client,
    );
    TestArgs {
        p2p_sync_server,
        storage_reader,
        header_sender,
        state_diff_sender,
        transaction_sender,
        class_sender,
        event_sender,
    }
}

use starknet_api::core::ClassHash;
fn insert_to_storage_test_blocks_up_to(storage_writer: &mut StorageWriter) {
    for i in 0..NUM_OF_BLOCKS {
        let block_number = BlockNumber(i.try_into().unwrap());
        let block_header = BlockHeader {
            block_hash: BlockHash(random::<u64>().into()),
            block_header_without_hash: BlockHeaderWithoutHash {
                block_number,
                ..Default::default()
            },
            ..Default::default()
        };
        storage_writer
            .begin_rw_txn()
            .unwrap()
            .append_header(block_number, &block_header)
            .unwrap()
            // TODO(shahak): Put different signatures for each block to test that we retrieve the
            // right signatures.
            .append_block_signature(block_number, &BlockSignature::default())
            .unwrap()
            .append_state_diff(block_number, THIN_STATE_DIFFS[i].clone())
            .unwrap()
            .append_body(block_number, BlockBody{transactions: TXS[i].clone(),
                transaction_outputs: TX_OUTPUTS[i].clone(),
                transaction_hashes: TX_HASHES[i].clone(),}).unwrap()
            .commit()
            .unwrap();
    }
}

// TODO(shahak): test undeclared class flow (when class manager client returns None).
fn create_mock_class_manager_with_blocks_up_to(
    storage_writer: &mut StorageWriter,
) -> SharedClassManagerClient {
    let mut class_manager_client = MockClassManagerClient::new();
    for i in 0..NUM_OF_BLOCKS {
        let block_number = BlockNumber(i.try_into().unwrap());
        let classes_with_hashes = CLASSES_WITH_HASHES[i]
            .iter()
            .map(|(class_hash, contract_class)| (*class_hash, contract_class))
            .collect::<Vec<_>>();

        for (class_hash, contract_class) in classes_with_hashes {
            class_manager_client
                .expect_get_sierra()
                .with(eq(class_hash))
                .returning(|_| Ok(Some(contract_class.clone())));
        }

        let deprecated_classes_with_hashes = DEPRECATED_CLASSES_WITH_HASHES[i]
            .iter()
            .map(|(class_hash, contract_class)| (*class_hash, contract_class))
            .collect::<Vec<_>>();

        for (class_hash, contract_class) in deprecated_classes_with_hashes {
            // TODO(shahak): test the case where class manager client returned error.
            class_manager_client
                .expect_get_executable()
                .with(eq(class_hash))
                .returning(|_| Ok(Some(ContractClass::V0(contract_class.clone()))));
        }

        storage_writer
            .begin_rw_txn()
            .unwrap()
            .update_class_manager_block_marker(&block_number.unchecked_next())
            .unwrap()
            .commit()
            .unwrap();
    }
    Arc::new(class_manager_client)
}

lazy_static! {
    static ref THIN_STATE_DIFFS: Vec<starknet_api::state::ThinStateDiff> = {
        let mut rng = get_rng();
        (0..NUM_OF_BLOCKS).map(|_| create_random_state_diff(&mut rng)).collect::<Vec<_>>()
    };
    static ref STATE_DIFF_CHUNCKS: Vec<StateDiffChunk> =
        THIN_STATE_DIFFS.iter().flat_map(|diff| split_thin_state_diff(diff.clone())).collect();
    static ref BODY: BlockBody =
        get_test_body(NUM_OF_BLOCKS * NUM_TXS_PER_BLOCK, Some(EVENTS_PER_TX), None, None);
    static ref TXS: Vec<Vec<Transaction>> =
        BODY.clone().transactions.chunks(NUM_TXS_PER_BLOCK).map(|chunk| chunk.to_vec()).collect();
    static ref TX_OUTPUTS: Vec<Vec<TransactionOutput>> = BODY
        .clone()
        .transaction_outputs
        .chunks(NUM_TXS_PER_BLOCK)
        .map(|chunk| chunk.to_vec())
        .collect();
    static ref TX_HASHES: Vec<Vec<TransactionHash>> = BODY
        .clone()
        .transaction_hashes
        .chunks(NUM_TXS_PER_BLOCK)
        .map(|chunk| chunk.to_vec())
        .collect();
    static ref EVENTS: Vec<Event> = TX_OUTPUTS
        .clone()
        .into_iter()
        .flat_map(|tx_output| tx_output.into_iter().flat_map(|output| output.events().to_vec()))
        .collect();
    static ref CLASSES_WITH_HASHES: Vec<Vec<(ClassHash, SierraContractClass)>> = {
        THIN_STATE_DIFFS
            .iter()
            .map(|state_diff| {
                let class_vec = state_diff
                    .class_hash_to_compiled_class_hash
                    .iter()
                    .map(|(class_hash, _)| {
                        (*class_hash, SierraContractClass::get_test_instance(&mut get_rng()))
                    })
                    .collect::<Vec<_>>();
                class_vec
            })
            .collect::<Vec<_>>()
    };
    static ref DEPRECATED_CLASSES_WITH_HASHES: Vec<Vec<(ClassHash, DeprecatedContractClass)>> = {
        THIN_STATE_DIFFS
            .iter()
            .map(|state_diff| {
                let deprecated_declared_classes_hashes =
                    state_diff.deprecated_declared_classes.clone();
                deprecated_declared_classes_hashes
                    .iter()
                    .map(|class_hash| {
                        (*class_hash, DeprecatedContractClass::get_test_instance(&mut get_rng()))
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
    };
}
