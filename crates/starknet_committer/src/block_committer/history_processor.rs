use std::sync::Arc;

use starknet_patricia_storage::storage_trait::{BlockNumber, Storage};
use tokio::runtime::Builder;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::{mpsc, Semaphore};
use tracing::info;

use crate::forest::filled_forest::FilledForest;

// Conctrols max concurrent DB write jobs.
pub const CONCURRENT_WORKERS: usize = 10;

pub struct HistoryProcessor {
    pub write_jobs_sender: mpsc::Sender<(BlockNumber, FilledForest)>,
    pub ack_receiver: mpsc::Receiver<BlockNumber>,
}

impl HistoryProcessor {
    pub fn new<S: Storage + Clone + Send + Sync + 'static>(db: &S) -> Self {
        let permits = Arc::new(Semaphore::new(CONCURRENT_WORKERS));
        let (write_jobs_sender, write_jobs_receiver) =
            mpsc::channel::<(BlockNumber, FilledForest)>(CONCURRENT_WORKERS);
        let (ack_sender, ack_receiver) = mpsc::channel::<BlockNumber>(CONCURRENT_WORKERS);

        tokio::spawn({
            let mut write_jobs_receiver = write_jobs_receiver;
            // We must own the db before moving it into the future oepration.
            let db = db.clone();
            async move {
                while let Some(batch) = write_jobs_receiver.recv().await {
                    // We must own the permit to send it to the blocking operation.
                    let permit = permits.clone().acquire_owned().await.unwrap();
                    // We need the DB throughout the channel's lifetime (while loop).
                    let mut db = db.clone();
                    let ack_sender = ack_sender.clone();

                    tokio::task::spawn_blocking(move || {
                        // Make sure the permit is not dropped before the blocking operation is
                        // completed.
                        let _permit = permit;
                        let (block_number, filled_forest) = batch;
                        let n_historical_facts =
                            filled_forest.write_to_storage(&mut db, Some(block_number));

                        let block_number = block_number.0;
                        info!(
                            "Written {n_historical_facts} facts to history for block \
                             {block_number}"
                        );

                        let _ = ack_sender.blocking_send(BlockNumber(block_number));
                    });
                }
            }
        });

        Self { write_jobs_sender, ack_receiver }
    }

    pub async fn submit(
        &self,
        block_number: BlockNumber,
        filled_forest: FilledForest,
    ) -> Result<(), SendError<(BlockNumber, FilledForest)>> {
        self.write_jobs_sender.send((block_number, filled_forest)).await
    }
}

pub fn build_tokio_runtime() -> tokio::runtime::Runtime {
    Builder::new_multi_thread()
        .max_blocking_threads(CONCURRENT_WORKERS)
        .enable_all()
        .build()
        .unwrap()
}
