use std::collections::{HashMap, HashSet};

use indexmap::IndexMap;
use starknet_api::block_hash::state_diff_hash::calculate_state_diff_hash;
use starknet_api::core::{ContractAddress, Nonce, StateDiffCommitment};
use starknet_api::execution_resources::GasAmount;
use starknet_api::state::ThinStateDiff;
use starknet_api::transaction::TransactionHash;

use crate::blockifier::transaction_executor::VisitedSegmentsMapping;
use crate::bouncer::BouncerWeights;
use crate::state::cached_state::CommitmentStateDiff;
use crate::transaction::objects::TransactionExecutionInfo;

#[cfg_attr(any(test, feature = "testing"), derive(Clone))]
#[derive(Debug, PartialEq)]
pub struct BlockExecutionArtifacts {
    pub execution_infos: IndexMap<TransactionHash, TransactionExecutionInfo>,
    pub commitment_state_diff: CommitmentStateDiff,
    pub visited_segments_mapping: VisitedSegmentsMapping,
    pub bouncer_weights: BouncerWeights,
    pub l2_gas_used: GasAmount,
}

impl BlockExecutionArtifacts {
    pub fn address_to_nonce(&self) -> HashMap<ContractAddress, Nonce> {
        HashMap::from_iter(
            self.commitment_state_diff
                .address_to_nonce
                .iter()
                .map(|(address, nonce)| (*address, *nonce)),
        )
    }

    pub fn tx_hashes(&self) -> HashSet<TransactionHash> {
        HashSet::from_iter(self.execution_infos.keys().copied())
    }

    pub fn state_diff(&self) -> ThinStateDiff {
        // TODO(Ayelet): Remove the clones.
        let storage_diffs = self.commitment_state_diff.storage_updates.clone();
        let nonces = self.commitment_state_diff.address_to_nonce.clone();
        ThinStateDiff {
            deployed_contracts: IndexMap::new(),
            storage_diffs,
            declared_classes: IndexMap::new(),
            nonces,
            // TODO: Remove this when the structure of storage diffs changes.
            deprecated_declared_classes: Vec::new(),
            replaced_classes: IndexMap::new(),
        }
    }

    pub fn commitment(&self) -> StateDiffCommitment {
        calculate_state_diff_hash(&self.state_diff())
    }
}
