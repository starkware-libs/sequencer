use std::cmp::min;

use futures::{FutureExt, StreamExt};
use papyrus_common::pending_classes::ApiContractClass;
use papyrus_protobuf::sync::{
    BlockHashOrNumber,
    ClassQuery,
    DataOrFin,
    DeclaredClass,
    DeprecatedDeclaredClass,
    Direction,
    Query,
    StateDiffChunk,
};
use papyrus_storage::class::ClassStorageReader;
use papyrus_test_utils::{get_rng, GetTestInstance};
use rand::{Rng, RngCore};
use rand_chacha::ChaCha8Rng;
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::state::ContractClass;

use super::test_utils::{
    setup,
    wait_for_marker,
    MarkerKind,
    TestArgs,
    CLASS_DIFF_QUERY_LENGTH,
    HEADER_QUERY_LENGTH,
    SLEEP_DURATION_TO_LET_SYNC_ADVANCE,
    TIMEOUT_FOR_TEST,
};
use crate::client::state_diff_test::run_state_diff_sync;

#[tokio::test]
async fn class_basic_flow() {
    let TestArgs {
        p2p_sync,
        storage_reader,
        mut mock_state_diff_response_manager,
        mut mock_header_response_manager,
        mut mock_class_response_manager,
        // The test will fail if we drop this
        mock_transaction_response_manager: _mock_transaction_responses_manager,
        ..
    } = setup();

    let mut rng = get_rng();
    let (class_state_diffs, api_contract_classes): (Vec<_>, Vec<_>) = (0..HEADER_QUERY_LENGTH)
        .map(|_| create_random_state_diff_chunk_with_class(&mut rng))
        .unzip();
    let header_state_diff_lengths =
        class_state_diffs.iter().map(|class_state_diff| class_state_diff.len()).collect::<Vec<_>>();

    // Create a future that will receive queries, send responses and validate the results
    let parse_queries_future = async move {
        // Check that before we send state diffs there is no class query.
        assert!(mock_class_response_manager.next().now_or_never().is_none());

        run_state_diff_sync(
            p2p_sync.config,
            &mut mock_header_response_manager,
            &mut mock_state_diff_response_manager,
            header_state_diff_lengths.clone(),
            class_state_diffs.clone().into_iter().map(Some).collect(),
        )
        .await;

        let num_declare_class_state_diff_headers =
            u64::try_from(header_state_diff_lengths.len()).unwrap();
        let num_class_queries =
            num_declare_class_state_diff_headers.div_ceil(CLASS_DIFF_QUERY_LENGTH);
        for i in 0..num_class_queries {
            let start_block_number = i * CLASS_DIFF_QUERY_LENGTH;
            let limit = min(
                num_declare_class_state_diff_headers - start_block_number,
                CLASS_DIFF_QUERY_LENGTH,
            );

            // Get a class query and validate it
            let mut mock_class_responses_manager =
                mock_class_response_manager.next().await.unwrap();
            assert_eq!(
                *mock_class_responses_manager.query(),
                Ok(ClassQuery(Query {
                    start_block: BlockHashOrNumber::Number(BlockNumber(start_block_number)),
                    direction: Direction::Forward,
                    limit,
                    step: 1,
                })),
                "If the limit of the query is too low, try to increase \
                 SLEEP_DURATION_TO_LET_SYNC_ADVANCE",
            );

            for block_number in start_block_number..(start_block_number + limit) {
                let class_hash =
                    class_state_diffs[usize::try_from(block_number).unwrap()].get_class_hash();
                let api_contract_class =
                    api_contract_classes[usize::try_from(block_number).unwrap()].clone();

                let block_number = BlockNumber(block_number);

                // Check that before we've sent all parts the state diff wasn't written yet
                let txn = storage_reader.begin_ro_txn().unwrap();
                assert_eq!(block_number, txn.get_class_marker().unwrap());

                mock_class_responses_manager
                    .send_response(DataOrFin(Some((api_contract_class.clone(), class_hash))))
                    .await
                    .unwrap();

                wait_for_marker(
                    MarkerKind::Class,
                    &storage_reader,
                    block_number.unchecked_next(),
                    SLEEP_DURATION_TO_LET_SYNC_ADVANCE,
                    TIMEOUT_FOR_TEST,
                )
                .await;

                let txn = storage_reader.begin_ro_txn().unwrap();
                let expected_class = match api_contract_class {
                    ApiContractClass::ContractClass(_) => ApiContractClass::ContractClass(
                        txn.get_class(&class_hash).unwrap().unwrap(),
                    ),
                    ApiContractClass::DeprecatedContractClass(_) => {
                        ApiContractClass::DeprecatedContractClass(
                            txn.get_deprecated_class(&class_hash).unwrap().unwrap(),
                        )
                    }
                };
                assert_eq!(api_contract_class, expected_class);
            }

            mock_class_responses_manager.send_response(DataOrFin(None)).await.unwrap();
        }
    };

    tokio::select! {
        sync_result = p2p_sync.run() => {
            sync_result.unwrap();
            panic!("P2P sync aborted with no failure.");
        }
        _ = parse_queries_future => {}
    }
}

trait GetClassHash {
    fn get_class_hash(&self) -> ClassHash;
}

impl GetClassHash for StateDiffChunk {
    fn get_class_hash(&self) -> ClassHash {
        match self {
            StateDiffChunk::DeclaredClass(declared_class) => declared_class.class_hash,
            StateDiffChunk::DeprecatedDeclaredClass(deprecated_declared_class) => {
                deprecated_declared_class.class_hash
            }
            _ => unreachable!(),
        }
    }
}

fn create_random_state_diff_chunk_with_class(
    rng: &mut ChaCha8Rng,
) -> (StateDiffChunk, ApiContractClass) {
    let class_hash = ClassHash(rng.next_u64().into());
    if rng.gen_bool(0.5) {
        let declared_class = DeclaredClass {
            class_hash,
            compiled_class_hash: CompiledClassHash(rng.next_u64().into()),
        };
        (
            StateDiffChunk::DeclaredClass(declared_class),
            ApiContractClass::ContractClass(ContractClass::get_test_instance(rng)),
        )
    } else {
        let deprecated_declared_class = DeprecatedDeclaredClass { class_hash };
        (
            StateDiffChunk::DeprecatedDeclaredClass(deprecated_declared_class),
            ApiContractClass::DeprecatedContractClass(DeprecatedContractClass::get_test_instance(
                rng,
            )),
        )
    }
}
