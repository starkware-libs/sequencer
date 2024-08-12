use std::collections::BTreeMap;
use std::sync::Arc;

use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_mempool_types::communication::{MempoolClientError, SharedMempoolClient};
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::{debug, error, info, instrument, Instrument};

// TODO: Should be defined in SN_API probably (shared with the consensus).
pub type ProposalId = u64;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProposalsManagerConfig {
    pub max_txs_per_mempool_request: usize,
}

impl Default for ProposalsManagerConfig {
    fn default() -> Self {
        // TODO: Get correct value for default max_txs_per_mempool_request.
        Self { max_txs_per_mempool_request: 10 }
    }
}

impl SerializeConfig for ProposalsManagerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([ser_param(
            "max_txs_per_mempool_request",
            &self.max_txs_per_mempool_request,
            "Maximum transactions to get from the mempool per iteration of proposal generation",
            ParamPrivacyInput::Public,
        )])
    }
}

#[derive(Debug, Error)]
pub enum ProposalsManagerError {
    #[error(
        "Received proposal generation request with id {new_proposal_id} while already generating \
         proposal with id {current_generating_proposal_id}."
    )]
    AlreadyGeneratingProposal {
        current_generating_proposal_id: ProposalId,
        new_proposal_id: ProposalId,
    },
    #[error(transparent)]
    MempoolError(#[from] MempoolClientError),
}

pub type ProposalsManagerResult<T> = Result<T, ProposalsManagerError>;

/// Main struct for handling block proposals.
/// Taking care of:
/// - Proposing new blocks.
/// - Validating incoming proposals.
/// - Commiting accepted proposals to the storage.
///
/// Triggered by the consensus.
// TODO: Remove dead_code attribute.
#[allow(dead_code)]
pub(crate) struct ProposalsManager {
    config: ProposalsManagerConfig,
    mempool_client: SharedMempoolClient,
    /// The block proposal that is currently being proposed, if any.
    /// At any given time, there can be only one proposal being actively executed (either proposed
    /// or validated).
    proposal_in_generation: Arc<Mutex<Option<ProposalId>>>,
    generation_task_handle: Option<tokio::task::JoinHandle<ProposalsManagerResult<()>>>,
}

impl ProposalsManager {
    // TODO: Remove dead_code attribute.
    #[allow(dead_code)]
    pub fn new(config: ProposalsManagerConfig, mempool_client: SharedMempoolClient) -> Self {
        Self {
            config,
            mempool_client,
            proposal_in_generation: Arc::new(Mutex::new(None)),
            generation_task_handle: None,
        }
    }

    /// Starts a new block proposal generation task for the given proposal_id and height with
    /// transactions from the mempool.
    // TODO: Understand why clippy is complaining about blocks_in_conditions.
    #[allow(clippy::blocks_in_conditions)]
    #[instrument(skip(self), err)]
    pub async fn generate_block_proposal(
        &mut self,
        proposal_id: ProposalId,
        _height: BlockNumber,
        _base_proposal_id: Option<ProposalId>,
    ) -> ProposalsManagerResult<()> {
        info!("Starting generation of new proposal.");
        self.set_proposal_in_generation(proposal_id).await?;
        let generate_block_task = Self::generate_block_task(
            self.mempool_client.clone(),
            self.proposal_in_generation.clone(),
            self.config.max_txs_per_mempool_request,
        );

        self.generation_task_handle = Some(tokio::spawn(generate_block_task.in_current_span()));
        Ok(())
    }

    // Checks if there is already a proposal being generated, and if not, sets the given proposal_id
    // as the one being generated.
    #[instrument(skip(self), err)]
    async fn set_proposal_in_generation(
        &mut self,
        proposal_id: ProposalId,
    ) -> ProposalsManagerResult<()> {
        let mut lock = self.proposal_in_generation.lock().await;

        if let Some(proposal_in_generation) = *lock {
            return Err(ProposalsManagerError::AlreadyGeneratingProposal {
                current_generating_proposal_id: proposal_in_generation,
                new_proposal_id: proposal_id,
            });
        }

        *lock = Some(proposal_id);
        debug!("Set proposal {} as the one being generated.", proposal_id);
        Ok(())
    }

    // Generates a block proposal with transactions from the mempool.
    // Not a passing &self method, because it's called from tokio::spawn.
    #[instrument(skip(mempool_client), err)]
    async fn generate_block_task(
        mempool_client: SharedMempoolClient,
        proposal_in_generation: Arc<Mutex<Option<ProposalId>>>,
        max_txs_per_mempool_request: usize,
    ) -> ProposalsManagerResult<()> {
        debug!("Starting a new block proposal generation task.");
        // TODO: notify the mempool that we are starting a new proposal generation.
        let block_builder = block_builder::BlockBuilder {};
        loop {
            let mempool_txs = mempool_client.get_txs(max_txs_per_mempool_request).await?;
            // TODO: Get L1 transactions.
            debug!("Adding {} mempool transactions to proposal in generation.", mempool_txs.len(),);
            // TODO: This is cpu bound operation, should use spawn_blocking / Rayon / std::thread
            // here or from inside the function.
            let is_block_ready = block_builder.add_txs(mempool_txs);
            if is_block_ready {
                break;
            }
            // Allow other tasks to run in case we are blocking the tokio runtime (when the mempool
            // is empty).
            tokio::task::yield_now().await;
        }
        // TODO: Get state diff.
        let mut proposal_id = proposal_in_generation.lock().await;
        *proposal_id = None;
        Ok(())
    }
}

// TODO: Should be defined elsewhere.
#[allow(dead_code)]
mod block_builder {
    use starknet_api::state::StateDiff;
    use starknet_mempool_types::mempool_types::ThinTransaction;

    #[derive(Debug, PartialEq)]
    pub enum Status {
        Building,
        Ready,
        Timeout,
    }

    pub struct BlockBuilder {}

    impl BlockBuilder {
        pub fn status(&self) -> Status {
            Status::Building
        }

        /// Returning true if the block is ready to be proposed.
        pub fn add_txs(&self, _txs: Vec<ThinTransaction>) -> bool {
            false
        }

        pub fn close_block(&self) -> StateDiff {
            StateDiff::default()
        }
    }
}
