use std::collections::{BTreeMap, HashMap};

use async_trait::async_trait;
use papyrus_config::dumping::SerializeConfig;
use papyrus_config::{ParamPath, SerializedParam};
use papyrus_storage::{StorageReader, StorageWriter};
use starknet_mempool_types::communication::SharedMempoolClient;
use tracing::instrument;

// TODO: Should be defined in SN_API probably (shared with the consensus).
pub type ProposalId = u64;

/// Public API for proposing new blocks.
#[async_trait]
pub trait ProposalGeneration {
    async fn start_new_proposal(&mut self, proposal_id: ProposalId);
}

// TODO: Add ProposalValidation
// TODO: Add ProposalCommitment

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ProposalsManagerConfig {}

impl SerializeConfig for ProposalsManagerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::new()
    }
}

// TODO: Use BuilderTask when it is available.
type Proposal = ();

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
    // TODO: Consider whether we need to open the storage by ourselves or we can get the handles
    // from outside.
    storage_reader: StorageReader,
    storage_writer: StorageWriter,
    mempool_client: SharedMempoolClient,
    proposals_in_generation: HashMap<ProposalId, Proposal>,
}

impl ProposalsManager {
    // TODO: Remove dead_code attribute.
    #[allow(dead_code)]
    pub fn new(
        config: ProposalsManagerConfig,
        storage_reader: StorageReader,
        storage_writer: StorageWriter,
        mempool_client: SharedMempoolClient,
    ) -> Self {
        Self {
            config,
            storage_reader,
            storage_writer,
            mempool_client,
            proposals_in_generation: HashMap::new(),
        }
    }
}

#[async_trait]
impl ProposalGeneration for ProposalsManager {
    #[instrument(skip(self))]
    async fn start_new_proposal(&mut self, proposal_id: ProposalId) {
        self.proposals_in_generation.insert(proposal_id, ());

        // TODO: Add proposal generation logic.
    }
}
