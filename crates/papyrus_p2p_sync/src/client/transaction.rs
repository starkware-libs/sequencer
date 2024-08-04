use futures::future::BoxFuture;
use futures::{FutureExt, StreamExt};
use papyrus_network::network_manager::ClientResponsesManager;
use papyrus_protobuf::sync::DataOrFin;
use papyrus_storage::body::{BodyStorageReader, BodyStorageWriter};
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::{StorageError, StorageReader, StorageWriter};
use rand::random;
use starknet_api::block::{BlockBody, BlockNumber};
use starknet_api::transaction::{Transaction, TransactionHash, TransactionOutput};
use starknet_types_core::felt::Felt;

use super::stream_builder::{BlockData, BlockNumberLimit, DataStreamBuilder};
use super::{P2PSyncClientError, NETWORK_DATA_TIMEOUT};

impl BlockData for (BlockBody, BlockNumber) {
    fn write_to_storage(
        self: Box<Self>,
        storage_writer: &mut StorageWriter,
    ) -> Result<(), StorageError> {
        storage_writer.begin_rw_txn()?.append_body(self.1, self.0)?.commit()
    }
}

pub(crate) struct TransactionStreamFactory;

impl DataStreamBuilder<(Transaction, TransactionOutput)> for TransactionStreamFactory {
    // TODO(Eitan): Add events protocol to BlockBody or split their write to storage
    type Output = (BlockBody, BlockNumber);

    const TYPE_DESCRIPTION: &'static str = "transactions";
    const BLOCK_NUMBER_LIMIT: BlockNumberLimit = BlockNumberLimit::HeaderMarker;

    fn parse_data_for_block<'a>(
        transactions_response_manager: &'a mut ClientResponsesManager<
            DataOrFin<(Transaction, TransactionOutput)>,
        >,
        block_number: BlockNumber,
        storage_reader: &'a StorageReader,
    ) -> BoxFuture<'a, Result<Option<Self::Output>, P2PSyncClientError>> {
        async move {
            let mut block_body = BlockBody::default();
            let mut current_transaction_len = 0;
            let target_transaction_len = storage_reader
                .begin_ro_txn()?
                .get_block_header(block_number)?
                .expect("A header with number lower than the header marker is missing")
                .n_transactions;
            while current_transaction_len < target_transaction_len {
                let maybe_transaction = tokio::time::timeout(
                    NETWORK_DATA_TIMEOUT,
                    transactions_response_manager.next(),
                )
                .await?
                .ok_or(P2PSyncClientError::ReceiverChannelTerminated {
                    type_description: Self::TYPE_DESCRIPTION,
                })?;
                let Some((transaction, transaction_output)) = maybe_transaction?.0 else {
                    if current_transaction_len == 0 {
                        return Ok(None);
                    } else {
                        return Err(P2PSyncClientError::NotEnoughTransactions {
                            expected: target_transaction_len,
                            actual: current_transaction_len,
                            block_number: block_number.0,
                        });
                    }
                };
                // TODO(eitan): Add transaction hash to protobuf
                let random_transaction_hash = TransactionHash(Felt::from(random::<u64>()));
                block_body.transactions.push(transaction);
                block_body.transaction_outputs.push(transaction_output);
                block_body.transaction_hashes.push(random_transaction_hash);
                current_transaction_len += 1;
            }
            Ok(Some((block_body, block_number)))
        }
        .boxed()
    }

    fn get_start_block_number(storage_reader: &StorageReader) -> Result<BlockNumber, StorageError> {
        storage_reader.begin_ro_txn()?.get_body_marker()
    }
}
