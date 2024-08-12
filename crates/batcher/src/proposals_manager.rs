use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
#[cfg(test)]
use mockall::automock;
use papyrus_config::converters::deserialize_milliseconds_to_duration;
use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_api::state::StateDiff;
use starknet_mempool_types::communication::{MempoolClient, MempoolClientError};
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::{debug, error, info, instrument, Instrument};

// TODO: Should be defined in SN_API probably (shared with the consensus).
pub type ProposalId = u64;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProposalsManagerConfig {
    pub max_txs_per_mempool_request: usize,
    #[serde(deserialize_with = "deserialize_milliseconds_to_duration")]
    pub sleep_time_between_add_txs_ms: std::time::Duration,
}

impl Default for ProposalsManagerConfig {
    fn default() -> Self {
        // TODO: Get correct value for default max_txs_per_mempool_request.
        Self {
            max_txs_per_mempool_request: 10,
            sleep_time_between_add_txs_ms: std::time::Duration::from_millis(100),
        }
    }
}

impl SerializeConfig for ProposalsManagerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "max_txs_per_mempool_request",
                &self.max_txs_per_mempool_request,
                "Maximum transactions to get from the mempool per iteration of proposal generation",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "sleep_time_between_add_txs_ms",
                &self.sleep_time_between_add_txs_ms.as_millis(),
                "Waiting time in milliseconds between fetching adding more transactions to \
                 building jobs.",
                ParamPrivacyInput::Public,
            ),
        ])
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
    #[error("Internal error, please check the logs.")]
    InternalError,
    #[error(transparent)]
    MempoolError(#[from] MempoolClientError),
}

pub type ProposalsManagerResult<T> = Result<T, ProposalsManagerError>;

#[cfg_attr(test, automock)]
#[async_trait]
pub trait ConsensusClient: Send + Sync {
    async fn finish(&self, proposal_id: ProposalId, state_diff: StateDiff);
}

/// Main struct for handling block proposals.
/// Taking care of:
/// - Proposing new blocks.
/// - Validating incoming proposals.
/// - Commiting accepted proposals to the storage.
///
/// Triggered by the consensus.
// TODO: Remove dead_code attribute.
#[allow(dead_code)]
pub(crate) struct ProposalsManager<M, C>
where
    M: MempoolClient,
    C: ConsensusClient,
{
    config: ProposalsManagerConfig,
    mempool_client: Arc<M>,
    consensus_client: Arc<C>,
    /// The proposal currently being generated, if any.
    proposal_in_generation: Arc<Mutex<Option<ProposalId>>>,
}

impl<M: MempoolClient + 'static, C: ConsensusClient + 'static> ProposalsManager<M, C> {
    // TODO: Remove dead_code attribute.
    #[allow(dead_code)]
    pub fn new(config: ProposalsManagerConfig, mempool_client: M, consensus_client: C) -> Self {
        Self {
            config,
            mempool_client: Arc::new(mempool_client),
            consensus_client: Arc::new(consensus_client),
            proposal_in_generation: Arc::new(Mutex::new(None)),
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
        _hight: BlockNumber,
        _base_proposa_id: Option<ProposalId>,
    ) -> ProposalsManagerResult<()> {
        info!("Starting generation of new proposal.");
        self.set_proposal_in_generation(proposal_id).await?;
        let generate_block_task = Self::generate_block_task(
            self.mempool_client.clone(),
            self.consensus_client.clone(),
            self.proposal_in_generation.clone(),
            self.config.max_txs_per_mempool_request,
            self.config.sleep_time_between_add_txs_ms,
        );

        // TODO: Should we return the task handle?
        tokio::spawn(generate_block_task.in_current_span());
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
    #[instrument(skip(mempool_client, consensus_client), err)]
    async fn generate_block_task(
        mempool_client: Arc<M>,
        consensus_client: Arc<C>,
        proposal_in_generation: Arc<Mutex<Option<ProposalId>>>,
        max_txs_per_mempool_request: usize,
        sleep_time_between_add_txs: std::time::Duration,
    ) -> ProposalsManagerResult<()> {
        debug!("Starting a new block proposal generation task.");
        let block_builder = block_builder::BlockBuilder::new(consensus_client.clone());
        // TODO: Should this code be here or in the block_builder?
        while block_builder.status() == block_builder::Status::Building {
            let mempool_txs = mempool_client.get_txs(max_txs_per_mempool_request).await?;
            // TODO: Get L1 transactions.
            debug!("Adding {} mempool transactions to proposal in generation.", mempool_txs.len(),);
            // TODO: This is cpu bound operation, should use spawn_blocking / Rayon / std::thread
            // here or from inside the function.
            block_builder.add_txs(mempool_txs);
            tokio::time::sleep(sleep_time_between_add_txs).await;
        }
        let state_diff = block_builder.close_block();
        let mut proposal_id = proposal_in_generation.lock().await;
        consensus_client
            .finish(proposal_id.expect("Should be the current proposal"), state_diff)
            .await;
        *proposal_id = None;
        Ok(())
    }
}

// TODO: Should be defined elsewhere.
#[allow(dead_code)]
mod block_builder {
    use std::sync::Arc;

    use starknet_api::state::StateDiff;
    use starknet_mempool_types::mempool_types::ThinTransaction;

    use super::ConsensusClient;

    #[derive(Debug, PartialEq)]
    pub enum Status {
        Building,
        Ready,
        Timeout,
    }

    pub struct BlockBuilder<C: ConsensusClient> {
        consensus_client: Arc<C>,
    }

    impl<C: ConsensusClient> BlockBuilder<C> {
        pub fn new(consensus_client: Arc<C>) -> Self {
            Self { consensus_client }
        }

        pub fn status(&self) -> Status {
            Status::Building
        }

        pub fn add_txs(&self, _txs: Vec<ThinTransaction>) {}

        pub fn close_block(&self) -> StateDiff {
            StateDiff::default()
        }
    }
}
