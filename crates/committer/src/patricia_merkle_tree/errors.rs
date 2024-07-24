use std::fmt::Debug;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum TypesError<T: Sized + Debug> {
    #[error("Failed to convert type {from:?} to {to}. Reason: {reason}.")]
    ConversionError { from: T, to: &'static str, reason: &'static str },
}
