use std::sync::Arc;

use async_trait::async_trait;
use futures::channel::oneshot;
use tokio::sync::Mutex;
use tokio::task::{self};
use tracing::debug;

// TODO(dvir): add tests when will have more logic.

/// A chunk of all the data to write to Aersopike.
#[derive(Debug)]
pub(crate) struct AerospikeBlob {
    // TODO(yael, dvir): add the blob fields.
}

#[async_trait]
pub(crate) trait CendeContext: Send + Sync {
    /// Write the previous height blob to Aerospike. Returns a cell with an inner boolean indicating
    /// whether the write was successful.
    fn write_prev_height_blob(&self) -> oneshot::Receiver<bool>;

    // Prepares the previous height blob that will be written in the next height.
    async fn prepare_blob_for_next_height(&self, blob_parameters: BlobParameters);
}

#[derive(Clone, Debug)]
pub(crate) struct CendeAmbassador {
    // TODO(dvir): consider creating enum varaiant instead of the `Option<AerospikeBlob>`.
    // `None` indicates that there is no blob to write, and therefore, the node can't be the
    // proposer.
    prev_height_blob: Arc<Mutex<Option<AerospikeBlob>>>,
}

impl CendeAmbassador {
    pub(crate) fn new() -> Self {
        CendeAmbassador { prev_height_blob: Arc::new(Mutex::new(None)) }
    }
}

#[async_trait]
impl CendeContext for CendeAmbassador {
    fn write_prev_height_blob(&self) -> oneshot::Receiver<bool> {
        let (sender, reciver) = oneshot::channel();
        let prev_height_blob = self.prev_height_blob.clone();
        task::spawn(async move {
            let Some(ref _blob) = *prev_height_blob.lock().await else {
                debug!("No blob to write to Aerospike.");
                sender.send(false).expect("Cell should be empty");
                return;
            };
            // TODO(dvir): write blob to AS.
            // TODO(dvir): consider set `prev_height_blob` to `None` after writing to AS.
            debug!("Writing blob to Aerospike.");
            sender.send(true).expect("Cell should be empty");
            debug!("Blob writing to Aerospike completed.");
        });

        reciver
    }

    async fn prepare_blob_for_next_height(&self, blob_parameters: BlobParameters) {
        // TODO(dvir, yael): make the full creation of blob.
        // TODO(dvir): as optimization, call the `into` and other preperation when writing to AS.
        *self.prev_height_blob.lock().await = Some(blob_parameters.into());
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct BlobParameters {
    // TODO(dvir): add here all the information needed for creating the blob: tranasctions, classes,
    // block info, BlockExecutionArtifacts.
}

impl From<BlobParameters> for AerospikeBlob {
    fn from(_blob_parameters: BlobParameters) -> Self {
        // TODO(yael): make the full creation of blob.
        AerospikeBlob {}
    }
}
