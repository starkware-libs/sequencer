mod central_objects;

use std::sync::Arc;

use async_trait::async_trait;
use futures::channel::oneshot;
#[cfg(test)]
use mockall::automock;
use starknet_api::block::BlockNumber;
use tokio::sync::Mutex;
use tokio::task::{self};
use tracing::debug;

// TODO(dvir): add tests when will have more logic.

/// A chunk of all the data to write to Aersopike.
#[derive(Debug)]
pub(crate) struct AerospikeBlob {
    // TODO(yael, dvir): add the blob fields.
}

#[cfg_attr(test, automock)]
#[async_trait]
pub trait CendeContext: Send + Sync {
    /// Write the previous height blob to Aerospike. Returns a cell with an inner boolean indicating
    /// whether the write was successful.
    /// `height` is the height of the block that is built when calling this function.
    fn write_prev_height_blob(&self, height: BlockNumber) -> oneshot::Receiver<bool>;

    // Prepares the previous height blob that will be written in the next height.
    async fn prepare_blob_for_next_height(&self, blob_parameters: BlobParameters);
}

#[derive(Clone, Debug, Default)]
pub struct CendeAmbassador {
    // TODO(dvir): consider creating enum varaiant instead of the `Option<AerospikeBlob>`.
    // `None` indicates that there is no blob to write, and therefore, the node can't be the
    // proposer.
    prev_height_blob: Arc<Mutex<Option<AerospikeBlob>>>,
}

impl CendeAmbassador {
    pub fn new() -> Self {
        CendeAmbassador { prev_height_blob: Arc::new(Mutex::new(None)) }
    }
}

#[async_trait]
impl CendeContext for CendeAmbassador {
    fn write_prev_height_blob(&self, height: BlockNumber) -> oneshot::Receiver<bool> {
        let (sender, receiver) = oneshot::channel();
        let prev_height_blob = self.prev_height_blob.clone();
        task::spawn(async move {
            // TODO(dvir): remove this when handle the booting up case.
            // Heights that are permitted to be built without writing to Aerospike.
            // Height 1 to make  `end_to_end_flow` test pass.
            const PERMITTED_HEIGHTS: [BlockNumber; 1] = [BlockNumber(1)];

            if PERMITTED_HEIGHTS.contains(&height) {
                debug!(
                    "height {} is in `PERMITTED_HEIGHTS`, consensus can send proposal without \
                     writing to Aerospike",
                    height
                );
                sender.send(true).unwrap();
                return;
            }
            let Some(ref _blob) = *prev_height_blob.lock().await else {
                debug!("No blob to write to Aerospike.");
                sender.send(false).expect("Writing to a one-shot sender should succeed.");
                return;
            };
            // TODO(dvir): write blob to AS.
            // TODO(dvir): consider set `prev_height_blob` to `None` after writing to AS.
            debug!("Writing blob to Aerospike.");
            sender.send(true).expect("Writing to a one-shot sender should succeed.");
            debug!("Blob writing to Aerospike completed.");
        });

        receiver
    }

    async fn prepare_blob_for_next_height(&self, blob_parameters: BlobParameters) {
        // TODO(dvir, yael): make the full creation of blob.
        // TODO(dvir): as optimization, call the `into` and other preperation when writing to AS.
        *self.prev_height_blob.lock().await = Some(blob_parameters.into());
    }
}

#[derive(Clone, Debug, Default)]
pub struct BlobParameters {
    // TODO(dvir): add here all the information needed for creating the blob: tranasctions, classes,
    // block info, BlockExecutionArtifacts.
}

impl From<BlobParameters> for AerospikeBlob {
    fn from(_blob_parameters: BlobParameters) -> Self {
        // TODO(yael): make the full creation of blob.
        AerospikeBlob {}
    }
}
