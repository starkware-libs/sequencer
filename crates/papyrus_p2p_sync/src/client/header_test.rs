use std::collections::HashMap;

use futures::future::BoxFuture;
use futures::FutureExt;
use papyrus_protobuf::sync::{BlockHashOrNumber, DataOrFin, Direction, Query, SignedBlockHeader};
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::StorageReader;
use papyrus_test_utils::get_rng;
use starknet_api::block::BlockNumber;

use super::test_utils::{
    random_header,
    run_test,
    wait_for_marker,
    Action,
    DataType,
    SLEEP_DURATION_TO_LET_SYNC_ADVANCE,
    TIMEOUT_FOR_TEST,
    WAIT_PERIOD_FOR_NEW_DATA,
};

#[tokio::test]
async fn signed_headers_basic_flow() {
    let (headers, check_storage_funcs) = create_headers_and_check_storage_funcs(4);
    run_test(
        HashMap::from([(DataType::Header, 2)]),
        vec![
            Action::ReceiveQuery(
                Box::new(|query| {
                    assert_eq!(
                        query,
                        Query {
                            start_block: BlockHashOrNumber::Number(BlockNumber(0)),
                            direction: Direction::Forward,
                            limit: 2,
                            step: 1,
                        }
                    )
                }),
                DataType::Header,
            ),
            Action::SendHeader(DataOrFin(Some(headers[0].clone()))),
            // We check the storage now to see that the sync writes each response before it parses
            // the next one.
            Action::CheckStorage(check_storage_funcs[0].clone()),
            Action::SendHeader(DataOrFin(Some(headers[1].clone()))),
            Action::SendHeader(DataOrFin(None)),
            // We check the storage now to see that the sync writes each response before it parses
            // the next one.
            Action::CheckStorage(check_storage_funcs[1].clone()),
            Action::ReceiveQuery(
                Box::new(|query| {
                    assert_eq!(
                        query,
                        Query {
                            start_block: BlockHashOrNumber::Number(BlockNumber(2)),
                            direction: Direction::Forward,
                            limit: 2,
                            step: 1,
                        }
                    )
                }),
                DataType::Header,
            ),
            Action::SendHeader(DataOrFin(Some(headers[2].clone()))),
            // We check the storage now to see that the sync writes each response before it parses
            // the next one.
            Action::CheckStorage(check_storage_funcs[2].clone()),
            Action::SendHeader(DataOrFin(Some(headers[3].clone()))),
            // We check the storage now to see that the sync writes each response before it parses
            // the next one.
            Action::CheckStorage(check_storage_funcs[3].clone()),
            Action::SendHeader(DataOrFin(None)),
        ],
    )
    .await;
}

#[tokio::test]
async fn sync_sends_new_header_query_if_it_got_partial_responses() {
    let (headers, check_storage_funcs) = create_headers_and_check_storage_funcs(3);
    run_test(
        HashMap::from([(DataType::Header, 2)]),
        vec![
            Action::ReceiveQuery(
                Box::new(|query| {
                    assert_eq!(
                        query,
                        Query {
                            start_block: BlockHashOrNumber::Number(BlockNumber(0)),
                            direction: Direction::Forward,
                            limit: 2,
                            step: 1,
                        }
                    )
                }),
                DataType::Header,
            ),
            Action::SendHeader(DataOrFin(Some(headers[0].clone()))),
            Action::CheckStorage(check_storage_funcs[0].clone()),
            Action::SendHeader(DataOrFin(None)),
            // Wait for the sync to enter sleep due to partial responses. Then, simulate time has
            // passed.
            Action::CheckStorage(Box::new(|_reader| {
                async move {
                    tokio::time::sleep(SLEEP_DURATION_TO_LET_SYNC_ADVANCE).await;
                    tokio::time::pause();
                    tokio::time::advance(WAIT_PERIOD_FOR_NEW_DATA).await;
                    tokio::time::resume();
                }
                .boxed()
            })),
            Action::ReceiveQuery(
                Box::new(|query| {
                    assert_eq!(
                        query,
                        Query {
                            start_block: BlockHashOrNumber::Number(BlockNumber(1)),
                            direction: Direction::Forward,
                            limit: 2,
                            step: 1,
                        }
                    )
                }),
                DataType::Header,
            ),
            Action::SendHeader(DataOrFin(Some(headers[1].clone()))),
            Action::CheckStorage(check_storage_funcs[1].clone()),
            Action::SendHeader(DataOrFin(Some(headers[2].clone()))),
            Action::CheckStorage(check_storage_funcs[2].clone()),
            Action::SendHeader(DataOrFin(None)),
        ],
    )
    .await;
}

#[tokio::test]
async fn wrong_block_number() {
    run_test(
        HashMap::from([(DataType::Header, 1)]),
        vec![
            // We already validate the query content in other tests.
            Action::ReceiveQuery(Box::new(|_query| ()), DataType::Header),
            Action::SendHeader(DataOrFin(Some(random_header(
                &mut get_rng(),
                BlockNumber(1),
                None,
                None,
            )))),
            Action::ValidateReportSent(DataType::Header),
            Action::CheckStorage(Box::new(|reader| {
                async move {
                    assert_eq!(0, reader.begin_ro_txn().unwrap().get_header_marker().unwrap().0);
                }
                .boxed()
            })),
        ],
    )
    .await;
}

// TODO(shahak): Add more negative tests.

fn create_headers_and_check_storage_funcs(
    num_headers: usize,
) -> (Vec<SignedBlockHeader>, Vec<Box<impl FnOnce(StorageReader) -> BoxFuture<'static, ()> + Clone>>)
{
    let headers = (0..num_headers)
        .map(|i| random_header(&mut get_rng(), BlockNumber(i.try_into().unwrap()), None, None))
        .collect::<Vec<_>>();

    let check_storage_funcs = headers
        .iter()
        .cloned()
        .enumerate()
        .map(|(i, expected_header)| {
            Box::new(move |storage_reader| {
                async move {
                    let block_number = BlockNumber(i.try_into().unwrap());
                    wait_for_marker(
                        DataType::Header,
                        &storage_reader,
                        block_number.unchecked_next(),
                        SLEEP_DURATION_TO_LET_SYNC_ADVANCE,
                        TIMEOUT_FOR_TEST,
                    )
                    .await;

                    let txn = storage_reader.begin_ro_txn().unwrap();
                    let actual_header = txn.get_block_header(block_number).unwrap().unwrap();
                    assert_eq!(actual_header, expected_header.block_header);
                }
                .boxed()
            })
        })
        .collect::<Vec<_>>();

    (headers, check_storage_funcs)
}
