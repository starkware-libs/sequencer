use derive_more::Display;
use serde::{Deserialize, Serialize};

use crate::errors::ConsensusManagerError;

#[derive(
    Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default, Display,
)]
pub struct ProposalId(pub u64);

// TODO(Tsabary/Matan): Populate the data structure used to invoke the consensus manager.
#[derive(Debug, Serialize, Deserialize)]
pub struct ConsensusManagerFnOneInput {}

// TODO(Tsabary/Matan): Populate the data structure used to invoke the consensus manager.
#[derive(Debug, Serialize, Deserialize)]
pub struct ConsensusManagerFnTwoInput {}

// TODO(Tsabary/Matan): Replace with the actual return type of the consensus manager function.
#[derive(Debug, Serialize, Deserialize)]
pub struct ConsensusManagerFnOneReturnValue {}

// TODO(Tsabary/Matan): Replace with the actual return type of the consensus manager function.
#[derive(Debug, Serialize, Deserialize)]
pub struct ConsensusManagerFnTwoReturnValue {}

pub type ConsensusManagerResult<T> = Result<T, ConsensusManagerError>;
