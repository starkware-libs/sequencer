use serde::{Deserialize, Serialize};

use crate::errors::BatcherError;

// TODO(Tsabary/Yael/Dafna): Populate the data structure used to invoke the batcher.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BatcherFnOneInput {}

// TODO(Tsabary/Yael/Dafna): Populate the data structure used to invoke the batcher.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BatcherFnTwoInput {}

// TODO(Tsabary/Yael/Dafna): Replace with the actual return type of the batcher function.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BatcherFnOneReturnValue {}

// TODO(Tsabary/Yael/Dafna): Replace with the actual return type of the batcher function.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BatcherFnTwoReturnValue {}

pub type BatcherResult<T> = Result<T, BatcherError>;
