#![allow(dead_code)]
use starknet_api::block::BlockNumber;
use starknet_api::core::GlobalRoot;

/// Output of commitment tasks.
pub(crate) struct CommitmentTaskOutput {
    pub(crate) global_root: GlobalRoot,
    pub(crate) height: BlockNumber,
}
