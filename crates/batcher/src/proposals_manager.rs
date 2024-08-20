use std::collections::BTreeMap;
use std::sync::Arc;

#[cfg(test)]
use mockall::automock;
use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_api::state::StateDiff;
use starknet_mempool_types::communication::{MempoolClientError, SharedMempoolClient};
use starknet_mempool_types::mempool_types::ThinTransaction;
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::{debug, error, info, instrument, Instrument, Level};

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
    #[error("Internal error, check the logs.")]
    InternalError,
    #[error(transparent)]
    MempoolError(#[from] MempoolClientError),
    #[error("No proposal is currently being generated.")]
    NoProposalInGeneration,
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
    // To be able to mock BlockBuilder in tests.
    block_builder_factory: Arc<dyn BlockBuilderFactory>,
    /// The block proposal that is currently being proposed, if any.
    /// At any given time, there can be only one proposal being actively executed (either proposed
    /// or validated).
    proposal_in_generation: Arc<Mutex<Option<ProposalId>>>,
    generation_task_handle:
        Option<tokio::task::JoinHandle<ProposalsManagerResult<(ProposalId, StateDiff)>>>,
}

impl ProposalsManager {
    // TODO: Remove dead_code attribute.
    #[allow(dead_code)]
    pub fn new(
        config: ProposalsManagerConfig,
        mempool_client: SharedMempoolClient,
        block_builder_factory: Arc<dyn BlockBuilderFactory>,
    ) -> Self {
        Self {
            config,
            mempool_client,
            proposal_in_generation: Arc::new(Mutex::new(None)),
            generation_task_handle: None,
            block_builder_factory,
        }
    }

    /// Starts a new block proposal generation task for the given proposal_id and height with
    /// transactions from the mempool.
    // TODO: Understand why clippy is complaining about blocks_in_conditions.
    #[allow(clippy::blocks_in_conditions)]
    #[instrument(skip(self), ret, err)]
    pub async fn generate_block_proposal(
        &mut self,
        proposal_id: ProposalId,
        _height: BlockNumber,
        _base_proposal_id: Option<ProposalId>,
    ) -> ProposalsManagerResult<()> {
        info!("Starting generation of new proposal.");
        self.set_proposal_in_generation(proposal_id).await?;
        // TODO: notify the mempool that we are starting a new proposal generation.
        let generate_block_task = GenerateBlockTask {
            mempool_client: self.mempool_client.clone(),
            proposal_in_generation: self.proposal_in_generation.clone(),
            max_txs_per_mempool_request: self.config.max_txs_per_mempool_request,
            block_builder_factory: self.block_builder_factory.clone(),
        };

        self.generation_task_handle =
            Some(tokio::spawn(generate_block_task.run().in_current_span()));
        // Allow the task to start.
        tokio::task::yield_now().await;
        Ok(())
    }

    /// Waits for the generation task to finish and returns the generated proposal.
    #[instrument(skip(self), ret(level = Level::TRACE), err)]
    pub async fn wait_for_ready_block(
        &mut self,
    ) -> ProposalsManagerResult<(ProposalId, StateDiff)> {
        let Some(task_handle) = self.generation_task_handle.take() else {
            debug!("No proposal is currently being generated.");
            return Err(ProposalsManagerError::NoProposalInGeneration);
        };
        task_handle.await.unwrap_or_else(|join_error| {
            error!("Error while waiting for the generation task to finish: {}", join_error);
            Err(ProposalsManagerError::InternalError)
        })
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
    #[instrument(skip(mempool_client, block_builder_factory), ret(level = Level::TRACE), err)]
    async fn generate_block_task(
        mempool_client: SharedMempoolClient,
        proposal_in_generation: Arc<Mutex<Option<ProposalId>>>,
        max_txs_per_mempool_request: usize,
        block_builder_factory: Arc<dyn BlockBuilderFactory>,
    ) -> ProposalsManagerResult<(ProposalId, StateDiff)> {
        debug!("Starting a new block proposal generation task.");
        // TODO: notify the mempool that we are starting a new proposal generation.
        let block_builder = block_builder_factory.create_block_builder();
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
        let Some(finished_proposal_id) = proposal_in_generation.lock().await.take() else {
            error!("proposal_in_generation is None during the task run.");
            return Err(ProposalsManagerError::InternalError);
        };

        debug!("Finished generating proposal with id {}.", finished_proposal_id);
        let state_diff = block_builder.get_state_diff();
        Ok((finished_proposal_id, state_diff))
    }
}

#[cfg_attr(test, automock)]
pub trait BlockBuilderTrait: Send {
    /// Returning true if the block is ready to be proposed.
    fn add_txs(&self, _txs: Vec<ThinTransaction>) -> bool;
    fn get_state_diff(&self) -> StateDiff;
}

#[allow(dead_code)]
#[cfg_attr(test, automock)]
pub(crate) trait BlockBuilderFactory: Send + Sync {
    fn create_block_builder(&self) -> Box<dyn BlockBuilderTrait>;
}

#[allow(dead_code)]
struct GenerateBlockTask {
    pub mempool_client: SharedMempoolClient,
    pub proposal_in_generation: Arc<Mutex<Option<ProposalId>>>,
    pub max_txs_per_mempool_request: usize,
    pub block_builder_factory: Arc<dyn BlockBuilderFactory>,
}

impl GenerateBlockTask {
    // Generates a block proposal with transactions from the mempool.
    #[instrument(skip(self), ret(level = Level::TRACE), err)]
    pub async fn run(self) -> ProposalsManagerResult<(ProposalId, StateDiff)> {
        debug!("Starting a new block proposal generation task.");
        let block_builder = self.block_builder_factory.create_block_builder();
        loop {
            let mempool_txs = self.mempool_client.get_txs(self.max_txs_per_mempool_request).await?;
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
        let Some(finished_proposal_id) = self.proposal_in_generation.lock().await.take() else {
            error!("proposal_in_generation is None during the task run.");
            return Err(ProposalsManagerError::InternalError);
        };

        debug!("Finished generating proposal with id {}.", finished_proposal_id);
        let state_diff = block_builder.get_state_diff();
        Ok((finished_proposal_id, state_diff))
    }
}
