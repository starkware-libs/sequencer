use rand::prelude::IteratorRandom;
use rand::rngs::SmallRng;
use starknet_committer::block_committer::input::{StarknetStorageKey, StateDiff};
use starknet_committer::block_committer::state_diff_generator::generate_random_state_diff;
use starknet_types_core::felt::Felt;

use crate::presets::types::flavors::BenchmarkFlavor;
use crate::utils::{
    leaf_preimages_to_storage_keys,
    FLAVOR_OVERLAP_WARMUP_BLOCKS,
    FLAVOR_PERIOD_MANY_WINDOW,
    FLAVOR_PERIOD_PERIOD,
};

impl BenchmarkFlavor {
    /// Returns the keys of the leaves that are updated in the given block.
    /// Depending on the flavor, some of the leaves to be updated are chosen randomly from the
    /// previous leaves, but all new leaf indices are deterministic.
    pub fn leaf_update_keys(
        &self,
        n_updates_arg: usize,
        block_number: usize,
        rng: &mut SmallRng,
    ) -> Vec<StarknetStorageKey> {
        let twenty_percent = n_updates_arg / 5;
        let total_leaves = self.total_nonzero_leaves_up_to(n_updates_arg, block_number);
        match self {
            Self::Constant => {
                leaf_preimages_to_storage_keys(total_leaves..(total_leaves + n_updates_arg))
            }
            Self::Continuous => (total_leaves..(total_leaves + n_updates_arg))
                .map(|i| StarknetStorageKey::try_from(Felt::from(i)).unwrap())
                .collect(),
            Self::Overlap => {
                // Invariant: if there are a total of L leaves in the DB, then the nonzero keys are
                // [hash(i) for i in 0..L].
                // Warmup phase: all leaves should be new, until 100M nonzero leaves exist.
                leaf_preimages_to_storage_keys(if block_number < FLAVOR_OVERLAP_WARMUP_BLOCKS {
                    // Warmup phase: all leaves should be new.
                    (total_leaves..(total_leaves + n_updates_arg)).collect()
                } else {
                    // We are warmed up, so only 20% of the leaves should be new.
                    // The total number of updates remains constant in this flavor.
                    // Sample (n_updates-new_leaves) old indices uniformly at random, from the
                    // previous leaves. Choose leaves from the (overlap_warmup_blocks * n_updates)
                    // most recent leaves.
                    let start_index = total_leaves - (FLAVOR_OVERLAP_WARMUP_BLOCKS * n_updates_arg);
                    let n_overlap_leaves = n_updates_arg - twenty_percent;
                    let updated_keys =
                        (start_index..total_leaves).choose_multiple(rng, n_overlap_leaves);
                    let new_keys = (total_leaves..(total_leaves + twenty_percent)).collect();
                    [updated_keys, new_keys].concat()
                })
            }
            Self::PeriodicPeaks => {
                let new_leaves = if block_number % FLAVOR_PERIOD_PERIOD < FLAVOR_PERIOD_MANY_WINDOW
                {
                    n_updates_arg
                } else {
                    twenty_percent
                };
                leaf_preimages_to_storage_keys(total_leaves..(total_leaves + new_leaves))
            }
        }
    }

    /// The nonzero leaf indices in the system are uniquely determined by the block number (see
    /// [Self::leaf_update_keys]), however, the actual state diff can be random depending on the
    /// flavor (nonzero leaf updates can be randomized).
    pub fn generate_state_diff(
        &self,
        n_updates_arg: usize,
        block_number: usize,
        rng: &mut SmallRng,
    ) -> StateDiff {
        let leaf_keys = self.leaf_update_keys(n_updates_arg, block_number, rng);
        let n_updates = leaf_keys.len();
        generate_random_state_diff(rng, n_updates, Some(leaf_keys))
    }
}
