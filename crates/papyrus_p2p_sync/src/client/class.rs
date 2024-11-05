use std::collections::HashSet;

use futures::future::BoxFuture;
use futures::{FutureExt, StreamExt};
use papyrus_common::pending_classes::ApiContractClass;
use papyrus_network::network_manager::ClientResponsesManager;
use papyrus_protobuf::sync::DataOrFin;
use papyrus_storage::class::ClassStorageWriter;
use papyrus_storage::state::StateStorageReader;
use papyrus_storage::{StorageError, StorageReader, StorageWriter};
use starknet_api::block::BlockNumber;
use starknet_api::core::ClassHash;
use starknet_api::state::{DeclaredClasses, DeprecatedDeclaredClasses};

use super::stream_builder::{
    BadPeerError,
    BlockData,
    BlockNumberLimit,
    DataStreamBuilder,
    ParseDataError,
};
use super::{P2PSyncClientError, NETWORK_DATA_TIMEOUT};

impl BlockData for (DeclaredClasses, DeprecatedDeclaredClasses, BlockNumber) {
    fn write_to_storage(
        self: Box<Self>,
        storage_writer: &mut StorageWriter,
    ) -> Result<(), StorageError> {
        storage_writer
            .begin_rw_txn()?
            .append_classes(
                self.2,
                &self.0.iter().map(|(class_hash, class)| (*class_hash, class)).collect::<Vec<_>>(),
                &self
                    .1
                    .iter()
                    .map(|(class_hash, deprecated_class)| (*class_hash, deprecated_class))
                    .collect::<Vec<_>>(),
            )?
            .commit()
    }
}

pub(crate) struct ClassStreamBuilder;

impl DataStreamBuilder<(ApiContractClass, ClassHash)> for ClassStreamBuilder {
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
                    tokio::time::timeout(NETWORK_DATA_TIMEOUT, classes_response_manager.next())
                        .await?
                        .ok_or(P2PSyncClientError::ReceiverChannelTerminated {
                            type_description: Self::TYPE_DESCRIPTION,
                        })?;
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
        storage_reader.begin_ro_txn()?.get_state_marker()
    }
}
