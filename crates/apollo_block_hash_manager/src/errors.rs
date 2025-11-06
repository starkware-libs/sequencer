use starknet_api::block::BlockNumber;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum BlockHashManagerError {
    #[error("Failed to set block commitment in batcher storage for height {0}.")]
    SetBlockCommitment(BlockNumber),
}
