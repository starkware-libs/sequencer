pub mod errors;

use crate::errors::L1ProviderError;

pub type L1ProviderResult<T> = Result<T, L1ProviderError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValidationStatus {
    Validated,
    AlreadyIncludedOnL2,
    ConsumedOnL1OrUnknown,
}
