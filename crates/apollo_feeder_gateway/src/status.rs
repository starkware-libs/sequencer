use apollo_starknet_client::reader::objects::block::BlockStatus;
use starknet_api::block::BlockNumber;

#[cfg(test)]
#[path = "status_test.rs"]
mod status_test;

/// Computes the legacy feeder gateway status of a finalized synced block.
///
/// `base_layer_marker` follows the `apollo_storage` marker convention: it is the first block
/// number NOT yet accepted on the base layer, so blocks strictly below it are `ACCEPTED_ON_L1` and
/// the rest are `ACCEPTED_ON_L2` (the Python default for finalized blocks).
///
/// `PENDING`/`ABORTED` apply only to non-finalized/reorged blocks, which synced storage does not
/// hold, and `PROVEN_ON_L2` is config-gated in Python (`enable_proven_on_l2_status`) with no
/// proven marker in synced storage to derive it from; none of them is produced here.
pub fn finalized_block_status(
    block_number: BlockNumber,
    base_layer_marker: BlockNumber,
) -> BlockStatus {
    if block_number < base_layer_marker {
        BlockStatus::AcceptedOnL1
    } else {
        BlockStatus::AcceptedOnL2
    }
}
