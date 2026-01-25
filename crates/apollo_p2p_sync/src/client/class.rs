use std::cmp::min;
use std::collections::{HashMap, HashSet};
use std::time::Duration;

use apollo_class_manager_types::SharedClassManagerClient;
use apollo_network::network_manager::{ClientResponsesManager, SqmrClientSender};
use apollo_protobuf::converters::CompressedApiContractClass;
use apollo_protobuf::sync::{BlockHashOrNumber, ClassQuery, DataOrFin, Direction, Query};
use apollo_state_sync_metrics::metrics::STATE_SYNC_CLASS_MANAGER_MARKER;
use apollo_state_sync_types::state_sync_types::SyncBlock;
use apollo_storage::class_manager::{ClassManagerStorageReader, ClassManagerStorageWriter};
use apollo_storage::state::StateStorageReader;
use apollo_storage::{StorageError, StorageReader, StorageWriter};
use async_stream::stream;
use futures::channel::mpsc::Receiver;
use futures::future::BoxFuture;
use futures::stream::BoxStream;
use futures::{FutureExt, StreamExt};
use papyrus_common::pending_classes::ApiContractClass;
use starknet_api::block::BlockNumber;
use starknet_api::core::ClassHash;
use starknet_api::state::{DeclaredClasses, DeprecatedDeclaredClasses};
use tracing::{debug, info, trace, warn};

use super::block_data_stream_builder::{
    BadPeerError,
    BlockData,
    BlockDataResult,
    BlockDataStreamBuilder,
    BlockNumberLimit,
    ParseDataError,
};
use super::{P2pSyncClientError, STEP};

impl BlockData for (DeclaredClasses, DeprecatedDeclaredClasses, BlockNumber) {
    fn write_to_storage<'a>(
        self: Box<Self>,
        storage_writer: &'a mut StorageWriter,
        class_manager_client: &'a mut SharedClassManagerClient,
    ) -> BoxFuture<'a, Result<(), P2pSyncClientError>> {
        async move {
            for (class_hash, class) in self.0 {
                // We can't continue without writing to the class manager, so we'll keep retrying
                // until it succeeds.
                // TODO(shahak): Test this flow.
                // TODO(shahak): Verify class hash matches class manager response. report if not.
                // TODO(shahak): Try to avoid cloning. See if ClientError can contain the request.
                while let Err(err) = class_manager_client.add_class(class.clone()).await {
                    warn!(
                        "Failed writing class with hash {class_hash:?} to class manager. Trying \
                         again. Error: {err:?}"
                    );
                    trace!("Class: {class:?}");
                    // TODO(shahak): Consider sleeping here.
                }
            }

            for (class_hash, deprecated_class) in self.1 {
                // TODO(shahak): Test this flow.
                // TODO(shahak): Try to avoid cloning. See if ClientError can contain the request.
                while let Err(err) = class_manager_client
                    .add_deprecated_class(class_hash, deprecated_class.clone())
                    .await
                {
                    warn!(
                        "Failed writing deprecated class with hash {class_hash:?} to class \
                         manager. Trying again. Error: {err:?}"
                    );
                    trace!("Class: {deprecated_class:?}");
                    // TODO(shahak): Consider sleeping here.
                }
            }

            storage_writer
                .begin_rw_txn()?
                .update_class_manager_block_marker(&self.2.unchecked_next())?
                .commit()?;
            STATE_SYNC_CLASS_MANAGER_MARKER.set_lossy(self.2.unchecked_next().0);

            Ok(())
        }
        .boxed()
    }
}

pub(crate) struct ClassStreamBuilder;

impl ClassStreamBuilder {
    /// Creates a stream for class data with configurable max Cairo0 program size.
    #[allow(clippy::too_many_arguments)]
    pub fn create_stream(
        mut sqmr_sender: SqmrClientSender<ClassQuery, DataOrFin<(CompressedApiContractClass, ClassHash)>>,
        storage_reader: StorageReader,
        mut internal_block_receiver: Option<Receiver<SyncBlock>>,
        wait_period_for_new_data: Duration,
        wait_period_for_other_protocol: Duration,
        num_blocks_per_query: u64,
        max_cairo0_program_size: usize,
    ) -> BoxStream<'static, BlockDataResult> {
        stream! {
            let mut current_block_number = Self::get_start_block_number(&storage_reader)?;
            let mut internal_blocks_received = HashMap::new();
            'send_query_and_parse_responses: loop {
                let limit = {
                    let last_block_number = storage_reader.begin_ro_txn()?.get_state_marker()?;
                    let limit = min(last_block_number.0 - current_block_number.0, num_blocks_per_query);
                    if limit == 0 {
                        trace!("{:?} sync is waiting for a new state diff", Self::TYPE_DESCRIPTION);
                        tokio::time::sleep(wait_period_for_other_protocol).await;
                        continue;
                    }
                    limit
                };
                if let Some(block) = Self::get_internal_block_at(&mut internal_blocks_received, &mut internal_block_receiver, current_block_number)
                    .now_or_never()
                {
                    info!("Added internally {:?} for block {}.", Self::TYPE_DESCRIPTION, current_block_number);
                    yield Ok(Box::<dyn BlockData>::from(Box::new(block)));
                    current_block_number = current_block_number.unchecked_next();
                    continue 'send_query_and_parse_responses;
                }

                let end_block_number = current_block_number.0 + limit;
                debug!(
                    "Sync sent query for {:?} for blocks [{}, {}) from network.",
                    Self::TYPE_DESCRIPTION,
                    current_block_number.0,
                    end_block_number,
                );

                let mut client_response_manager = sqmr_sender
                    .send_new_query(
                        ClassQuery::from(Query {
                            start_block: BlockHashOrNumber::Number(current_block_number),
                            direction: Direction::Forward,
                            limit,
                            step: STEP,
                        })
                    ).await?;
                while current_block_number.0 < end_block_number {
                    tokio::select! {
                        res = Self::parse_data_for_block_with_config(
                            &mut client_response_manager, current_block_number, &storage_reader, max_cairo0_program_size
                        ) => {
                            match res {
                                Ok(Some(output)) => {
                                    info!("Added {:?} for block {}.", Self::TYPE_DESCRIPTION, current_block_number);
                                    current_block_number = current_block_number.unchecked_next();
                                    yield Ok(Box::<dyn BlockData>::from(Box::new(output)));
                                }
                                Ok(None) => {
                                    debug!(
                                        "Query for {:?} on {:?} returned with partial data. Waiting {:?} before \
                                         sending another query.",
                                        Self::TYPE_DESCRIPTION, current_block_number, wait_period_for_new_data
                                    );
                                    tokio::time::sleep(wait_period_for_new_data).await;
                                    continue 'send_query_and_parse_responses;
                                },
                                Err(ParseDataError::BadPeer(err)) => {
                                    warn!(
                                        "Query for {:?} on {:?} returned with bad peer error: {:?}. reporting \
                                         peer and retrying query.",
                                        Self::TYPE_DESCRIPTION, current_block_number, err
                                    );
                                    client_response_manager.report_peer();
                                    continue 'send_query_and_parse_responses;
                                },
                                Err(ParseDataError::Fatal(err)) => {
                                    yield Err(err);
                                    return;
                                },
                            }
                        }
                        block = Self::get_internal_block_at(&mut internal_blocks_received, &mut internal_block_receiver, current_block_number) => {
                                info!("Added internally {:?} for block {}.", Self::TYPE_DESCRIPTION, current_block_number);
                                current_block_number = current_block_number.unchecked_next();
                                yield Ok(Box::<dyn BlockData>::from(Box::new(block)));
                                debug!("Network query ending at block {} for {:?} being ignored due to internal block", end_block_number, Self::TYPE_DESCRIPTION);
                                continue 'send_query_and_parse_responses;
                            }
                        }
                    }

                // Consume the None message signaling the end of the query.
                match client_response_manager.next().await {
                    Some(Ok(DataOrFin(None))) => {
                        debug!("Network query ending at block {} for {:?} finished", end_block_number, Self::TYPE_DESCRIPTION);
                    },
                    Some(_) => {
                        warn!(
                            "Query for {:?} returned more messages after {:?} even though it \
                            should have returned Fin. reporting peer and retrying query.",
                            Self::TYPE_DESCRIPTION, current_block_number
                        );
                        client_response_manager.report_peer();
                        continue 'send_query_and_parse_responses;
                    }

                    None => {
                        warn!(
                            "Query for {:?} didn't send Fin after block {:?}. \
                            Reporting peer and retrying query.",
                            Self::TYPE_DESCRIPTION, current_block_number
                        );
                        client_response_manager.report_peer();
                        continue 'send_query_and_parse_responses;
                    }
                }
            }
        }.boxed()
    }

    fn parse_data_for_block_with_config<'a>(
        classes_response_manager: &'a mut ClientResponsesManager<
            DataOrFin<(CompressedApiContractClass, ClassHash)>,
        >,
        block_number: BlockNumber,
        storage_reader: &'a StorageReader,
        max_cairo0_program_size: usize,
    ) -> BoxFuture<'a, Result<Option<(DeclaredClasses, DeprecatedDeclaredClasses, BlockNumber)>, ParseDataError>> {
        async move {
            let (target_class_len, declared_classes, deprecated_declared_classes) = {
                let state_diff = storage_reader
                    .begin_ro_txn()?
                    .get_state_diff(block_number)?
                    .expect("A state diff with number lower than the state diff marker is missing");
                (
                    state_diff.class_hash_to_compiled_class_hash.len()
                        + state_diff.deprecated_declared_classes.len(),
                    state_diff.class_hash_to_compiled_class_hash,
                    state_diff.deprecated_declared_classes.iter().cloned().collect::<HashSet<_>>(),
                )
            };
            let (
                mut current_class_len,
                mut declared_classes_result,
                mut deprecated_declared_classes_result,
            ) = (0, DeclaredClasses::new(), DeprecatedDeclaredClasses::new());

            while current_class_len < target_class_len {
                let maybe_contract_class = classes_response_manager.next().await.ok_or(
                    ParseDataError::BadPeer(BadPeerError::SessionEndedWithoutFin {
                        type_description: Self::TYPE_DESCRIPTION,
                    }),
                )?;
                let Some((compressed_class, class_hash)) = maybe_contract_class?.0 else {
                    if current_class_len == 0 {
                        return Ok(None);
                    } else {
                        return Err(ParseDataError::BadPeer(BadPeerError::NotEnoughClasses {
                            expected: target_class_len,
                            actual: current_class_len,
                            block_number: block_number.0,
                        }));
                    }
                };

                let api_contract_class = compressed_class.decompress(max_cairo0_program_size)?;

                let (is_declared, duplicate_class) = match api_contract_class {
                    ApiContractClass::ContractClass(contract_class) => (
                        declared_classes.get(&class_hash).is_some(),
                        declared_classes_result.insert(class_hash, contract_class).is_some(),
                    ),
                    ApiContractClass::DeprecatedContractClass(deprecated_contract_class) => (
                        deprecated_declared_classes.contains(&class_hash),
                        deprecated_declared_classes_result
                            .insert(class_hash, deprecated_contract_class)
                            .is_some(),
                    ),
                };

                if !is_declared {
                    return Err(ParseDataError::BadPeer(BadPeerError::ClassNotInStateDiff {
                        class_hash,
                    }));
                }

                if duplicate_class {
                    return Err(ParseDataError::BadPeer(BadPeerError::DuplicateClass {
                        class_hash,
                    }));
                }

                current_class_len += 1;
            }
            Ok(Some((declared_classes_result, deprecated_declared_classes_result, block_number)))
        }
        .boxed()
    }

    fn get_start_block_number(storage_reader: &StorageReader) -> Result<BlockNumber, StorageError> {
        storage_reader.begin_ro_txn()?.get_class_manager_block_marker()
    }

    fn get_internal_block_at<'a>(
        internal_blocks_received: &'a mut HashMap<BlockNumber, (DeclaredClasses, DeprecatedDeclaredClasses, BlockNumber)>,
        internal_block_receiver: &'a mut Option<Receiver<SyncBlock>>,
        current_block_number: BlockNumber,
    ) -> BoxFuture<'a, (DeclaredClasses, DeprecatedDeclaredClasses, BlockNumber)> {
        async move {
            if let Some(block) = internal_blocks_received.remove(&current_block_number) {
                return block;
            }
            let internal_block_receiver =
                internal_block_receiver.as_mut().expect("Internal block receiver not set");
            while let Some(sync_block) = internal_block_receiver.next().await {
                let block_number = sync_block.block_header_without_hash.block_number;
                if block_number >= current_block_number {
                    let block_data = Self::convert_sync_block_to_block_data(block_number, sync_block);
                    if block_number == current_block_number {
                        return block_data;
                    }
                    internal_blocks_received.insert(block_number, block_data);
                }
            }
            panic!("Internal block receiver terminated");
        }
        .boxed()
    }

    fn convert_sync_block_to_block_data(
        block_number: BlockNumber,
        _sync_block: SyncBlock,
    ) -> (DeclaredClasses, DeprecatedDeclaredClasses, BlockNumber) {
        (DeclaredClasses::new(), DeprecatedDeclaredClasses::new(), block_number)
    }

    const TYPE_DESCRIPTION: &'static str = "classes";
}

impl BlockDataStreamBuilder<(CompressedApiContractClass, ClassHash)> for ClassStreamBuilder {
    type Output = (DeclaredClasses, DeprecatedDeclaredClasses, BlockNumber);

    const TYPE_DESCRIPTION: &'static str = "classes";
    const BLOCK_NUMBER_LIMIT: BlockNumberLimit = BlockNumberLimit::StateDiffMarker;

    fn parse_data_for_block<'a>(
        classes_response_manager: &'a mut ClientResponsesManager<
            DataOrFin<(CompressedApiContractClass, ClassHash)>,
        >,
        block_number: BlockNumber,
        storage_reader: &'a StorageReader,
    ) -> BoxFuture<'a, Result<Option<Self::Output>, ParseDataError>> {
        // Delegate to the config version with the default value.
        Self::parse_data_for_block_with_config(
            classes_response_manager,
            block_number,
            storage_reader,
            apollo_p2p_sync_config::config::DEFAULT_MAX_CAIRO0_PROGRAM_SIZE,
        )
    }

    fn get_start_block_number(storage_reader: &StorageReader) -> Result<BlockNumber, StorageError> {
        storage_reader.begin_ro_txn()?.get_class_manager_block_marker()
    }

    // Returning empty set because we assume that internal block's classes are already added to the
    // class manager by the caller.
    fn convert_sync_block_to_block_data(
        block_number: BlockNumber,
        _sync_block: SyncBlock,
    ) -> (DeclaredClasses, DeprecatedDeclaredClasses, BlockNumber) {
        (DeclaredClasses::new(), DeprecatedDeclaredClasses::new(), block_number)
    }
}
