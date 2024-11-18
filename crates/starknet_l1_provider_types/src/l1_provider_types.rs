use crate::errors::L1ProviderError;

pub type L1ProviderResult<T> = Result<T, L1ProviderError>;
