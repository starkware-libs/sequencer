use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error, Serialize, Deserialize, Clone)]
pub enum MempoolP2pPropagatorError {
    #[error("Sender request error")]
    NetworkSendError,
}
