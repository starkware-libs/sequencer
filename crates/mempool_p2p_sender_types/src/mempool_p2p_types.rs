use crate::errors::MempoolP2pSenderError;

pub type MempoolP2pSenderResult<T> = Result<T, MempoolP2pSenderError>;
