pub mod errors;

use crate::errors::L1GasPriceProviderError;

pub type L1GasPriceProviderResult<T> = Result<T, L1GasPriceProviderError>;
