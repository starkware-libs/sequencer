use serde::{Deserialize, Serialize};

use crate::errors::BatcherError;

// TODO(Tsabary/Yael/Dafna): Populate the data structure used to invoke the batcher.
#[derive(Debug, Serialize, Deserialize)]
pub struct BatcherInput {}

pub type BatcherResult<T> = Result<T, BatcherError>;
