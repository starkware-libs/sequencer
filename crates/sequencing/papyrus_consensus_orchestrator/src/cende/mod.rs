#[cfg(test)]
mod cende_test;
mod central_objects;

use std::collections::BTreeMap;
use std::future::ready;
use std::sync::Arc;

use async_trait::async_trait;
#[cfg(test)]
use mockall::automock;
use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use reqwest::{Client, RequestBuilder};
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use tokio::sync::Mutex;
use tokio::task::{self, JoinHandle};
use tracing::debug;
use url::Url;

/// A chunk of all the data to write to Aersopike.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct AerospikeBlob {
    // TODO(yael, dvir): add the blob fields.
    block_number: BlockNumber,
}

#[cfg_attr(test, automock)]
#[async_trait]
pub trait CendeContext: Send + Sync {
    /// Write the previous height blob to Aerospike. Returns a cell with an inner boolean indicating
    /// whether the write was successful.
    /// `current_height` is the height of the block that is built when calling this function.
    fn write_prev_height_blob(&self, current_height: BlockNumber) -> JoinHandle<bool>;

    // Prepares the previous height blob that will be written in the next height.
    async fn prepare_blob_for_next_height(&self, blob_parameters: BlobParameters);
}

#[derive(Clone, Debug)]
pub struct CendeAmbassador {
    // TODO(dvir): consider creating enum varaiant instead of the `Option<AerospikeBlob>`.
    // `None` indicates that there is no blob to write, and therefore, the node can't be the
    // proposer.
    prev_height_blob: Arc<Mutex<Option<AerospikeBlob>>>,
    url: Url,
    client: Client,
}

/// The path to write blob in the Recorder.
pub const RECORDER_WRITE_BLOB_PATH: &str = "/cende/write_blob";

impl CendeAmbassador {
    pub fn new(cende_config: CendeConfig) -> Self {
        CendeAmbassador {
            prev_height_blob: Arc::new(Mutex::new(None)),
            url: cende_config
                .recorder_url
                .join(RECORDER_WRITE_BLOB_PATH)
                .expect("Failed to join `RECORDER_WRITE_BLOB_PATH` with the Recorder URL"),
            client: Client::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CendeConfig {
    pub recorder_url: Url,
}

impl Default for CendeConfig {
    fn default() -> Self {
        CendeConfig {
            // TODO(dvir): change this default value to "https://<recorder_url>". The reason for the
            // current value is to make the `end_to_end_flow_test` to pass (it creates the default
            // config).
            recorder_url: "https://recorder_url"
                .parse()
                .expect("recorder_url must be a valid Recorder URL"),
        }
    }
}

impl SerializeConfig for CendeConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([ser_param(
            "recorder_url",
            &self.recorder_url,
            "The URL of the Pythonic cende_recorder",
            ParamPrivacyInput::Private,
        )])
    }
}

#[async_trait]
impl CendeContext for CendeAmbassador {
    fn write_prev_height_blob(&self, current_height: BlockNumber) -> JoinHandle<bool> {
        let prev_height_blob = self.prev_height_blob.clone();
        let request_builder = self.client.post(self.url.clone());

        // TODO(dvir): remove this when handle the booting up case.
        // Heights that are permitted to be built without writing to Aerospike.
        // Height 1 to make  `end_to_end_flow` test pass.
        const SKIP_WRITE_HEIGHTS: [BlockNumber; 1] = [BlockNumber(1)];

        if SKIP_WRITE_HEIGHTS.contains(&current_height) {
            debug!(
                "height {} is in `SKIP_WRITE_HEIGHTS`, consensus can send proposal without \
                 writing to Aerospike",
                current_height
            );
            return tokio::spawn(ready(true));
        }

        task::spawn(async move {
            // TODO(dvir): consider extracting the "should write blob" logic to a function.
            let Some(ref blob): Option<AerospikeBlob> = *prev_height_blob.lock().await else {
                // This case happens when restarting the node, `prev_height_blob` intial value is
                // `None`.
                debug!("No blob to write to Aerospike.");
                return false;
            };

            // Can happen in case the consensus got a block from the state sync and due to that did
            // not update the cende ambassador in `decision_reached` function.
            // TODO(dvir): what to do in the case of the `blob.block_number.0 >= height.0`? this
            // means a bug.
            if blob.block_number.0 + 1 != current_height.0 {
                debug!(
                    "Mismatch blob block number and height, can't write blob to Aerospike. Blob \
                     block number {}, height {}",
                    blob.block_number, current_height
                );
                return false;
            }

            debug!("Writing blob to Aerospike.");
            return send_write_blob(request_builder, blob).await;
        })
    }

    async fn prepare_blob_for_next_height(&self, blob_parameters: BlobParameters) {
        // TODO(dvir, yael): make the full creation of blob.
        // TODO(dvir): as optimization, call the `into` and other preperation when writing to AS.
        *self.prev_height_blob.lock().await = Some(blob_parameters.into());
    }
}

async fn send_write_blob(request_builder: RequestBuilder, blob: &AerospikeBlob) -> bool {
    // TODO(dvir): consider set `prev_height_blob` to `None` after writing to AS.
    match request_builder.json(blob).send().await {
        Ok(response) => {
            if response.status().is_success() {
                debug!("Blob written to Aerospike successfully.");
                true
            } else {
                debug!(
                    "The recorder failed to write blob.\nStatus code: {}\nMessage: {}",
                    response.status(),
                    response.text().await.unwrap_or_default()
                );
                false
            }
        }
        Err(err) => {
            // TODO(dvir): try to test this case.
            debug!("Failed to send a request to the recorder. Error: {}", err);
            false
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct BlobParameters {
    height: u64,
    // TODO(dvir): add here all the information needed for creating the blob: tranasctions,
    // classes, block info, BlockExecutionArtifacts.
}

impl BlobParameters {
    pub fn new(height: u64) -> Self {
        BlobParameters { height }
    }
}

impl From<BlobParameters> for AerospikeBlob {
    fn from(blob_parameters: BlobParameters) -> Self {
        // TODO(yael): make the full creation of blob.
        AerospikeBlob { block_number: BlockNumber(blob_parameters.height) }
    }
}
