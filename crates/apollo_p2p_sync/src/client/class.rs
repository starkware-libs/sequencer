use std::collections::HashSet;

use apollo_class_manager_types::SharedClassManagerClient;
use apollo_network::network_manager::ClientResponsesManager;
use apollo_protobuf::sync::DataOrFin;
use apollo_state_sync_metrics::metrics::STATE_SYNC_CLASS_MANAGER_MARKER;
use apollo_state_sync_types::state_sync_types::SyncBlock;
use apollo_storage::class_manager::{ClassManagerStorageReader, ClassManagerStorageWriter};
use apollo_storage::state::StateStorageReader;
use apollo_storage::{StorageError, StorageReader, StorageWriter};
use futures::future::BoxFuture;
use futures::{FutureExt, StreamExt};
use papyrus_common::pending_classes::ApiContractClass;
use starknet_api::block::BlockNumber;
use starknet_api::core::ClassHash;
use starknet_api::state::{DeclaredClasses, DeprecatedDeclaredClasses};
use tracing::{trace, warn};

use super::block_data_stream_builder::{
    BadPeerError,
    BlockData,
    BlockDataStreamBuilder,
    BlockNumberLimit,
    ParseDataError,
};
use super::P2pSyncClientError;
use crate::client::RESPONSE_TIMEOUT;

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

impl BlockDataStreamBuilder<(ApiContractClass, ClassHash)> for ClassStreamBuilder {
    type Output = (DeclaredClasses, DeprecatedDeclaredClasses, BlockNumber);

    const TYPE_DESCRIPTION: &'static str = "classes";
    const BLOCK_NUMBER_LIMIT: BlockNumberLimit = BlockNumberLimit::StateDiffMarker;

    fn parse_data_for_block<'a>(
        classes_response_manager: &'a mut ClientResponsesManager<
            DataOrFin<(ApiContractClass, ClassHash)>,
        >,
        block_number: BlockNumber,
        storage_reader: &'a StorageReader,
    ) -> BoxFuture<'a, Result<Option<Self::Output>, ParseDataError>> {
        async move {
            let (target_class_len, declared_classes, deprecated_declared_classes) = {
                let state_diff = storage_reader
                    .begin_ro_txn()?
                    .get_state_diff(block_number)?
                    .expect("A state diff with number lower than the state diff marker is missing");
                (
                    state_diff.declared_classes.len()
                        + state_diff.deprecated_declared_classes.len(),
                    state_diff.declared_classes,
                    state_diff.deprecated_declared_classes.iter().cloned().collect::<HashSet<_>>(),
                )
            };
            let (
                mut current_class_len,
                mut declared_classes_result,
                mut deprecated_declared_classes_result,
            ) = (0, DeclaredClasses::new(), DeprecatedDeclaredClasses::new());

            while current_class_len < target_class_len {
                let maybe_contract_class =
                    tokio::time::timeout(RESPONSE_TIMEOUT, classes_response_manager.next())
                        .await
                        .map_err(|_| ParseDataError::BadPeer(BadPeerError::ResponseTimeout))?
                        .ok_or(ParseDataError::BadPeer(BadPeerError::SessionEndedWithoutFin {
                            type_description: Self::TYPE_DESCRIPTION,
                        }))?;
                let Some((api_contract_class, class_hash)) = maybe_contract_class?.0 else {
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

    // Returning empty set because we assume that internal block's classes are already added to the
    // class manager by the caller.
    fn convert_sync_block_to_block_data(
        block_number: BlockNumber,
        _sync_block: SyncBlock,
    ) -> (DeclaredClasses, DeprecatedDeclaredClasses, BlockNumber) {
        (DeclaredClasses::new(), DeprecatedDeclaredClasses::new(), block_number)
    }
}
