use futures::future::BoxFuture;
use futures::{FutureExt, StreamExt};
use papyrus_network::network_manager::ClientResponsesManager;
use papyrus_protobuf::sync::DataOrFin;
use papyrus_storage::body::{BodyStorageReader, BodyStorageWriter};
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::{StorageError, StorageReader, StorageWriter};
use papyrus_test_utils::{get_rng, GetTestInstance};
use starknet_api::block::{BlockBody, BlockNumber};
use starknet_api::transaction::{FullTransaction, Transaction, TransactionOutput};
use starknet_class_manager_types::SharedClassManagerClient;
use starknet_state_sync_types::state_sync_types::SyncBlock;

use super::block_data_stream_builder::{
    BadPeerError,
    BlockData,
    BlockDataStreamBuilder,
    BlockNumberLimit,
    ParseDataError,
};
use super::{P2pSyncClientError, NETWORK_DATA_TIMEOUT};

impl BlockData for (BlockBody, BlockNumber) {
    fn write_to_storage<'a>(
        self: Box<Self>,
        storage_writer: &'a mut StorageWriter,
        _class_manager_client: &'a mut SharedClassManagerClient,
    ) -> BoxFuture<'a, Result<(), P2pSyncClientError>> {
        async move {
            storage_writer.begin_rw_txn()?.append_body(self.1, self.0)?.commit()?;
            Ok(())
        }
        .boxed()
    }
}

pub(crate) struct TransactionStreamFactory;

impl BlockDataStreamBuilder<FullTransaction> for TransactionStreamFactory {
    // TODO(Eitan): Add events protocol to BlockBody or split their write to storage
    type Output = (BlockBody, BlockNumber);

    const TYPE_DESCRIPTION: &'static str = "transactions";
    const BLOCK_NUMBER_LIMIT: BlockNumberLimit = BlockNumberLimit::HeaderMarker;

    fn parse_data_for_block<'a>(
        transactions_response_manager: &'a mut ClientResponsesManager<DataOrFin<FullTransaction>>,
        block_number: BlockNumber,
        storage_reader: &'a StorageReader,
    ) -> BoxFuture<'a, Result<Option<Self::Output>, ParseDataError>> {
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
                .ok_or(P2pSyncClientError::ReceiverChannelTerminated {
                    type_description: Self::TYPE_DESCRIPTION,
                })?;
                let Some(FullTransaction { transaction, transaction_output, transaction_hash }) =
                    maybe_transaction?.0
                else {
                    if current_transaction_len == 0 {
                        return Ok(None);
                    } else {
                        return Err(ParseDataError::BadPeer(BadPeerError::NotEnoughTransactions {
                            expected: target_transaction_len,
                            actual: current_transaction_len,
                            block_number: block_number.0,
                        }));
                    }
                };
                block_body.transactions.push(transaction);
                block_body.transaction_outputs.push(transaction_output);
                // TODO(eitan): Validate transaction hash from untrusted sources
                block_body.transaction_hashes.push(transaction_hash);
                current_transaction_len += 1;
            }
            Ok(Some((block_body, block_number)))
        }
        .boxed()
    }

    fn get_start_block_number(storage_reader: &StorageReader) -> Result<BlockNumber, StorageError> {
        storage_reader.begin_ro_txn()?.get_body_marker()
    }

    // TODO(Eitan): Use real transactions once SyncBlock contains data required by full nodes
    fn convert_sync_block_to_block_data(
        block_number: BlockNumber,
        sync_block: SyncBlock,
    ) -> (BlockBody, BlockNumber) {
        let num_transactions = sync_block.transaction_hashes.len();
        let mut rng = get_rng();
        let block_body = BlockBody {
            transaction_hashes: sync_block.transaction_hashes,
            transaction_outputs: std::iter::repeat_with(|| {
                TransactionOutput::get_test_instance(&mut rng)
            })
            .take(num_transactions)
            .collect::<Vec<_>>(),
            transactions: std::iter::repeat_with(|| Transaction::get_test_instance(&mut rng))
                .take(num_transactions)
                .collect::<Vec<_>>(),
        };
        (block_body, block_number)
    }
}
