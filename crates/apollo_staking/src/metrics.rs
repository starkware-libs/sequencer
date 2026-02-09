use std::collections::HashSet;

use apollo_metrics::define_metrics;
use apollo_metrics::metrics::LossyIntoF64;
use metrics::gauge;

use crate::committee_provider::StakerSet;

define_metrics!(
    Staking => {
        // Epoch state.
        MetricGauge { STAKING_CURRENT_EPOCH_ID, "staking_current_epoch_id", "Current epoch ID" },
        MetricGauge { STAKING_CURRENT_EPOCH_START_BLOCK, "staking_current_epoch_start_block", "Start block of current epoch" },
        MetricGauge { STAKING_CURRENT_EPOCH_LENGTH, "staking_current_epoch_length", "Length of current epoch in blocks" },

        // Committee composition.
        MetricGauge { STAKING_COMMITTEE_TOTAL_WEIGHT, "staking_committee_total_weight", "Total voting weight of committee" },
        MetricGauge { STAKING_COMMITTEE_ELIGIBLE_PROPOSERS_TOTAL_WEIGHT, "staking_committee_eligible_proposers_total_weight", "Total weight of eligible proposers" },
    },
);

pub fn register_metrics() {
    STAKING_CURRENT_EPOCH_ID.register();
    STAKING_CURRENT_EPOCH_START_BLOCK.register();
    STAKING_CURRENT_EPOCH_LENGTH.register();
    STAKING_COMMITTEE_TOTAL_WEIGHT.register();
    STAKING_COMMITTEE_ELIGIBLE_PROPOSERS_TOTAL_WEIGHT.register();
}

/// Manages per-staker weight metrics for the committee.
/// Creates a separate gauge for each committee member with their address as a label.
#[derive(Default)]
pub struct CommitteeMemberMetrics {
    // Track which addresses we've seen to clean up old metrics.
    active_addresses: HashSet<String>,
}

impl CommitteeMemberMetrics {
    pub fn new() -> Self {
        Self { active_addresses: HashSet::new() }
    }

    /// Updates the committee member metrics.
    /// Sets gauges for current committee members and clears old ones.
    pub fn update_committee_members(&mut self, members: &StakerSet) {
        let mut new_addresses = HashSet::new();

        // Set gauge for each current member.
        for staker in members {
            let address = format!("{:#x}", staker.address.0.key());
            gauge!("staking_committee_member_weight", "address" => address.clone())
                .set(staker.weight.0.into_f64());
            new_addresses.insert(address);
        }

        // Clear metrics for addresses no longer in the committee.
        for old_address in self.active_addresses.difference(&new_addresses) {
            gauge!("staking_committee_member_weight", "address" => old_address.clone()).set(0.0);
        }

        self.active_addresses = new_addresses;
    }
}
