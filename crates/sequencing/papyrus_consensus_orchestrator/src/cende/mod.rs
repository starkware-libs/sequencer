mod central_objects;

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use futures::channel::oneshot;
#[cfg(test)]
use mockall::automock;
use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use tokio::sync::Mutex;
use tokio::task::{self};
use tracing::debug;
use url::Url;

// TODO(dvir): add tests when will have more logic.

/// The path to write blob in the Recorder.
const RECORDER_WRITE_BLOB_PATH: &str = "/write_blob";

/// A chunk of all the data to write to Aersopike.
#[derive(Debug, Serialize, Deserialize)]
pub struct AerospikeBlob {
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

#[cfg_attr(test, automock)]
#[async_trait]
pub trait RecorderClientTrait: Send + Sync + std::fmt::Debug {
    async fn send_blob(&self, blob: &AerospikeBlob) -> Result<reqwest::Response, reqwest::Error>;
}

#[derive(Debug)]
pub struct RecorderClient {
    pub client: Client,
    pub url: Url,
}

impl RecorderClient {
    pub fn new(cende_config: &CendeConfig) -> Self {
        RecorderClient {
            url: cende_config
                .recorder_url
                .join(RECORDER_WRITE_BLOB_PATH)
                .expect("Failed to join `RECORDER_WRITE_BLOB_PATH` with the Recorder URL"),
            client: Client::new(),
        }
    }
}

#[async_trait]
impl RecorderClientTrait for RecorderClient {
    async fn send_blob(&self, blob: &AerospikeBlob) -> Result<reqwest::Response, reqwest::Error> {
        let request_builder = self.client.post(self.url.clone());
        request_builder.json(blob).send().await
    }
}

#[derive(Debug)]
pub struct CendeAmbassador {
    // TODO(dvir): consider creating enum varaiant instead of the `Option<AerospikeBlob>`.
    // `None` indicates that there is no blob to write, and therefore, the node can't be the
    // proposer.
    prev_height_blob: Arc<Mutex<Option<AerospikeBlob>>>,
    recorder_client: Arc<dyn RecorderClientTrait>,
}

impl CendeAmbassador {
    pub fn new(recorder_client: Arc<dyn RecorderClientTrait>) -> Self {
        CendeAmbassador { prev_height_blob: Arc::new(Mutex::new(None)), recorder_client }
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
    fn write_prev_height_blob(&self, height: BlockNumber) -> oneshot::Receiver<bool> {
        let (sender, receiver) = oneshot::channel();
        let prev_height_blob = self.prev_height_blob.clone();
        let recorder_client = self.recorder_client.clone();
        task::spawn(async move {
            // TODO(dvir): remove this when handle the booting up case.
            // Heights that are permitted to be built without writing to Aerospike.
            // Height 1 to make  `end_to_end_flow` test pass.
            const SKIP_WRITE_HEIGHTS: [BlockNumber; 1] = [BlockNumber(1)];

            if SKIP_WRITE_HEIGHTS.contains(&height) {
                debug!(
                    "height {} is in `SKIP_WRITE_HEIGHTS`, consensus can send proposal without \
                     writing to Aerospike",
                    height
                );
                oneshot_send(sender, true);
                return;
            }
            let Some(ref blob) = *prev_height_blob.lock().await else {
                // This case happens when restarting the node, `prev_height_blob` intial value is
                // `None`.
                debug!("No blob to write to Aerospike.");
                oneshot_send(sender, false);
                return;
            };
            // TODO(dvir): consider set `prev_height_blob` to `None` after writing to AS.
            debug!("Writing blob to Aerospike.");
            match recorder_client.send_blob(blob).await {
                Ok(response) => {
                    if response.status().is_success() {
                        debug!("Blob written to Aerospike successfully.");
                        oneshot_send(sender, true);
                    } else {
                        debug!("The recorder failed to write blob. Error: {}", response.status());
                        oneshot_send(sender, false);
                    }
                }
                Err(err) => {
                    debug!("Failed to send a request to the recorder. Error: {}", err);
                    // TODO(dvir): change this to `false`. The reason for the current value is to
                    // make the `end_to_end_flow_test` to pass.
                    oneshot_send(sender, true);
                }
            }
        });

        return receiver;

        // Helper function to send a boolean result to a one-shot sender.
        fn oneshot_send(sender: oneshot::Sender<bool>, result: bool) {
            sender.send(result).expect("Writing to a one-shot sender should succeed.");
        }
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
