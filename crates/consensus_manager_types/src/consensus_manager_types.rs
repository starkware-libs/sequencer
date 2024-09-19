use std::fmt::Debug;

use serde::{Deserialize, Serialize};

use crate::errors::ConsensusManagerError;

// TODO(Tsabary/Matan): Populate the data structure used to invoke the consensus manager.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConsensusManagerFnOneInput {}

// TODO(Tsabary/Matan): Populate the data structure used to invoke the consensus manager.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConsensusManagerFnTwoInput {}

// TODO(Tsabary/Matan): Replace with the actual return type of the consensus manager function.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConsensusManagerFnOneReturnValue {}

// TODO(Tsabary/Matan): Replace with the actual return type of the consensus manager function.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConsensusManagerFnTwoReturnValue {}

pub type ConsensusManagerResult<T> = Result<T, ConsensusManagerError>;
