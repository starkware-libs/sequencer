use blake2::digest::consts::U31;
use blake2::{Blake2s, Digest};
use starknet_committer::block_committer::input::StarknetStorageKey;
use starknet_types_core::felt::Felt;

use crate::presets::types::flavors::BenchmarkFlavor;

pub const FLAVOR_PERIOD_MANY_WINDOW: usize = 10;
pub const FLAVOR_PERIOD_PERIOD: usize = 500;

pub const FLAVOR_OVERLAP_WARMUP_BLOCKS: usize = 100_000;

/// Given a range, generates pseudorandom 31-byte storage keys hashed from the numbers in the range.
pub fn leaf_preimages_to_storage_keys(
    indices: impl IntoIterator<Item = usize>,
) -> Vec<StarknetStorageKey> {
    indices
        .into_iter()
        .map(|i| {
            let mut hasher = Blake2s::<U31>::new();
            hasher.update(i.to_be_bytes().as_slice());
            let result = hasher.finalize();
            StarknetStorageKey::try_from(Felt::from_bytes_be_slice(result.as_slice())).unwrap()
        })
        .collect()
}

impl BenchmarkFlavor {
    /// Returns the total amount of nonzero leaves in the system up to (not including) the block
    /// number.
    pub fn total_nonzero_leaves_up_to(&self, n_updates_arg: usize, block_number: usize) -> usize {
        let twenty_percent = n_updates_arg / 5;
        match self {
            Self::Constant | Self::Continuous => block_number * n_updates_arg,
            Self::Overlap => {
                if block_number < FLAVOR_OVERLAP_WARMUP_BLOCKS {
                    block_number * n_updates_arg
                } else {
                    FLAVOR_OVERLAP_WARMUP_BLOCKS * n_updates_arg
                        + (block_number - FLAVOR_OVERLAP_WARMUP_BLOCKS) * twenty_percent
                }
            }
            Self::PeriodicPeaks => {
                let updates_per_period = n_updates_arg * FLAVOR_PERIOD_MANY_WINDOW
                    + twenty_percent * (FLAVOR_PERIOD_PERIOD - FLAVOR_PERIOD_MANY_WINDOW);
                let mod_period = block_number % FLAVOR_PERIOD_PERIOD;
                let is_many_window = mod_period < FLAVOR_PERIOD_MANY_WINDOW;

                let total_leaves_added_in_period = if is_many_window {
                    // We are still in the initial window with many updates.
                    n_updates_arg * mod_period
                } else {
                    // We have passed the many-updates window.
                    n_updates_arg * FLAVOR_PERIOD_MANY_WINDOW
                        + twenty_percent * (mod_period - FLAVOR_PERIOD_MANY_WINDOW)
                };
                (block_number / FLAVOR_PERIOD_PERIOD) * updates_per_period
                    + total_leaves_added_in_period
            }
        }
    }
}
