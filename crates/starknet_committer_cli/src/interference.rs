use rand::distributions::Uniform;
use rand::rngs::SmallRng;
use rand::Rng;
use starknet_patricia_storage::storage_trait::{AsyncStorage, DbKey};
use tokio::task::JoinSet;

use crate::presets::types::flavors::{BenchmarkFlavor, InterferenceFlavor};
use crate::utils::leaf_preimages_to_storage_keys;

const INTERFERENCE_READ_1K_EVERY_BLOCK_N_READS: usize = 1000;

pub async fn apply_interference<S: AsyncStorage>(
    interference_type: &InterferenceFlavor,
    benchmark_flavor: &BenchmarkFlavor,
    n_updates_arg: usize,
    block_number: usize,
    task_set: &mut JoinSet<()>,
    mut storage: S,
    rng: &mut SmallRng,
) {
    match interference_type {
        InterferenceFlavor::None => {}
        InterferenceFlavor::Read1KEveryBlock => {
            let total_leaves =
                benchmark_flavor.total_nonzero_leaves_up_to(n_updates_arg, block_number + 1);
            // Avoid creating an iterator over the entire range - select random leaves, with
            // possible repetition. Probability of repitition will decrease as the number of
            // leaves increases.
            let dist = Uniform::new(0, total_leaves);
            let preimages = (0..INTERFERENCE_READ_1K_EVERY_BLOCK_N_READS)
                .map(|_| rng.sample(dist))
                .collect::<Vec<_>>();
            task_set.spawn(async move {
                let keys = leaf_preimages_to_storage_keys(preimages)
                    .iter()
                    .map(|k| DbKey((**k.0).to_bytes_be().to_vec()))
                    .collect::<Vec<_>>();
                storage.mget(&keys.iter().collect::<Vec<&DbKey>>()).await.unwrap();
            });
        }
    }
}
