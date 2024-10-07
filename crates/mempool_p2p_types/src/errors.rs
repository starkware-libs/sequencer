use serde::{Deserialize, Serialize};
use thiserror::Error;

// This error is defined even though it's empty to be compatible with the other components.
#[derive(Debug, Error, Serialize, Deserialize, Clone)]
pub enum MempoolP2pSenderError {
    #[error("Sender request error")]
    NetworkSendError,
}
