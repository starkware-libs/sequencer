use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use tokio::sync::Mutex;
use tokio::task::{self};
use tracing::debug;

// TODO(dvir): add tests when will have more logic.

/// A chunk of all the data to write to Aersopike.
#[derive(Clone, Debug)]
pub(crate) struct AerospikeBlob {
    // TODO(yael, dvir): add the blob fields.
}

#[async_trait]
pub(crate) trait CendeContext: Send + Sync {
    /// Write the previous height blob to Aerospike. Returns a cell with an inner boolean indicating
    /// whether the write was successful.
    fn write_prev_height_blob(&self) -> Arc<OnceLock<bool>>;

    // Prepares the previous height blob that will be written in the next height.
    async fn prepare_prev_height_blob(&self, nfb: NeededForBlob);
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
    fn write_prev_height_blob(&self) -> Arc<OnceLock<bool>> {
        let cell = Arc::new(OnceLock::new());
        let prev_height_blob = self.prev_height_blob.clone();
        let cloned_cell = cell.clone();
        task::spawn(async move {
            if let Some(_blob) = prev_height_blob.lock().await.clone() {
                // TODO(dvir): write blob to AS.
                debug!("Writing blob to AS.");
                cell.set(true).expect("Cell should be empty");
                debug!("Blob writing to AS completed.");
            } else {
                debug!("No blob to write to AS.");
                cell.set(false).expect("Cell should be empty");
            }
        });
        cloned_cell
    }

    async fn prepare_prev_height_blob(&self, nfb: NeededForBlob) {
        // TODO(dvir, yael): make the full creation of blob.
        // TODO(dvir): as optimization, call the `into` and other preperation when writing to AS.
        *self.prev_height_blob.lock().await = Some(nfb.into());
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct NeededForBlob {
    // TODO(dvir): add here all the information needed for creating the blob: tranasctions, classes,
    // block info, BlockExecutionArtifacts.
}

impl From<NeededForBlob> for AerospikeBlob {
    fn from(_nfb: NeededForBlob) -> Self {
        // TODO(yael): make the full creation of blob.
        AerospikeBlob {}
    }
}
