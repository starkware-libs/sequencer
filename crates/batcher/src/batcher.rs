use std::collections::HashMap;
use std::sync::Arc;

#[cfg(test)]
use mockall::automock;
use papyrus_storage::state::StateStorageReader;
use starknet_api::block::BlockNumber;
use starknet_api::executable_transaction::Transaction;
use starknet_batcher_types::batcher_types::{
    BatcherResult,
    BuildProposalInput,
    GetProposalContent,
    GetProposalContentInput,
    GetProposalContentResponse,
    ProposalId,
    StartHeightInput,
};
use starknet_batcher_types::errors::BatcherError;
use starknet_mempool_infra::component_definitions::ComponentStarter;
use starknet_mempool_types::communication::SharedMempoolClient;
use tracing::{error, instrument};

use crate::config::BatcherConfig;
use crate::proposal_manager::{
    BlockBuilderFactory,
    BuildProposalError,
    ProposalManager,
    ProposalManagerTrait,
    StartHeightError,
};

struct Proposal {
    content_stream: ProposalContentStream,
}

pub struct Batcher {
    pub config: BatcherConfig,
    pub storage: Arc<dyn BatcherStorageReaderTrait>,
    proposal_manager: Box<dyn ProposalManagerTrait>,
    proposals: HashMap<ProposalId, Proposal>,
}

impl Batcher {
    pub(crate) fn new(
        config: BatcherConfig,
        storage: Arc<dyn BatcherStorageReaderTrait>,
        proposal_manager: Box<dyn ProposalManagerTrait>,
    ) -> Self {
        Self {
            config: config.clone(),
            storage: storage.clone(),
            proposal_manager,
            proposals: HashMap::new(),
        }
    }

    pub fn start_height(&mut self, input: StartHeightInput) -> BatcherResult<()> {
        self.proposals.clear();
        self.proposal_manager.start_height(input.height).map_err(BatcherError::from)
    }

    #[instrument(skip(self), err)]
    pub async fn build_proposal(
        &mut self,
        build_proposal_input: BuildProposalInput,
    ) -> BatcherResult<()> {
        let proposal_id = build_proposal_input.proposal_id;
        if self.proposals.contains_key(&proposal_id) {
            return Err(BatcherError::ProposalAlreadyExists { proposal_id });
        }
        let deadline =
            tokio::time::Instant::from_std(build_proposal_input.deadline_as_instant().map_err(
                |_| BatcherError::TimeToDeadlineError { deadline: build_proposal_input.deadline },
            )?);

        let (tx_sender, tx_receiver) = tokio::sync::mpsc::unbounded_channel();

        self.proposal_manager
            .build_block_proposal(build_proposal_input.proposal_id, deadline, tx_sender)
            .await
            .map_err(BatcherError::from)?;

        let content_stream = ProposalContentStream::BuildProposal(tx_receiver);
        self.proposals.insert(proposal_id, Proposal { content_stream });
        Ok(())
    }

    pub async fn get_proposal_content(
        &mut self,
        get_proposal_content_input: GetProposalContentInput,
    ) -> BatcherResult<GetProposalContentResponse> {
        let proposal_id = get_proposal_content_input.proposal_id;
        let proposal = self
            .proposals
            .get_mut(&proposal_id)
            .ok_or(BatcherError::ProposalNotFound { proposal_id })?;

        let ProposalContentStream::BuildProposal(stream) = &mut proposal.content_stream else {
            return Err(BatcherError::GetContentOnNonBuildProposal { proposal_id });
        };
        let mut txs = Vec::new();
        if stream.recv_many(&mut txs, self.config.outstream_content_buffer_size).await == 0 {
            // TODO: send `Finished` and then exhausted.
            return Err(BatcherError::StreamExhausted);
        }

        Ok(GetProposalContentResponse { content: GetProposalContent::Txs(txs) })
    }
}

pub fn create_batcher(config: BatcherConfig, mempool_client: SharedMempoolClient) -> Batcher {
    let (storage_reader, _storage_writer) = papyrus_storage::open_storage(config.storage.clone())
        .expect("Failed to open batcher's storage");
    let storage_reader = Arc::new(storage_reader);
    let proposal_manager = Box::new(ProposalManager::new(
        config.proposal_manager.clone(),
        mempool_client.clone(),
        Arc::new(BlockBuilderFactory {}),
        storage_reader.clone(),
    ));
    Batcher::new(config, storage_reader, proposal_manager)
}

#[cfg_attr(test, automock)]
pub trait BatcherStorageReaderTrait: Send + Sync {
    fn height(&self) -> papyrus_storage::StorageResult<BlockNumber>;
}

impl BatcherStorageReaderTrait for papyrus_storage::StorageReader {
    fn height(&self) -> papyrus_storage::StorageResult<BlockNumber> {
        self.begin_ro_txn()?.get_state_marker()
    }
}

pub(crate) enum ProposalContentStream {
    BuildProposal(OutputStream),
    // TODO: Add stream.
    #[allow(dead_code)]
    ValidateProposal,
}

// TODO: Make this work with streams.
type OutputStream = tokio::sync::mpsc::UnboundedReceiver<Transaction>;

impl From<StartHeightError> for BatcherError {
    fn from(err: StartHeightError) -> Self {
        match err {
            StartHeightError::AlreadyWorkingOnHeight { active_height, new_height } => {
                BatcherError::AlreadyWorkingOnHeight { active_height, new_height }
            }
            StartHeightError::HeightAlreadyPassed { storage_height, requested_height } => {
                BatcherError::HeightAlreadyPassed { storage_height, requested_height }
            }
            StartHeightError::StorageError(err) => {
                error!("{}", err);
                BatcherError::InternalError
            }
            StartHeightError::StorageNotSynced { storage_height, requested_height } => {
                BatcherError::StorageNotSynced { storage_height, requested_height }
            }
        }
    }
}

impl From<BuildProposalError> for BatcherError {
    fn from(err: BuildProposalError) -> Self {
        match err {
            BuildProposalError::AlreadyGeneratingProposal {
                current_generating_proposal_id,
                new_proposal_id,
            } => BatcherError::ServerBusy {
                active_proposal_id: current_generating_proposal_id,
                new_proposal_id,
            },
            BuildProposalError::MempoolError(..) => BatcherError::InternalError,
            BuildProposalError::NoActiveHeight => BatcherError::NoActiveHeight,
        }
    }
}

impl ComponentStarter for Batcher {}
