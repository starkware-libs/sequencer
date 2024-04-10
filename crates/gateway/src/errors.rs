use starknet_api::transaction::{Resource, ResourceBounds};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum GatewayError {
    #[error(transparent)]
    HTTPError(#[from] hyper::http::Error),
    #[error("Internal server error")]
    InternalServerError,
    #[error(transparent)]
    InvalidTransactionFormat(#[from] serde_json::Error),
    #[error("Error while starting the server")]
    ServerStartError(#[from] hyper::Error),
}

#[derive(Debug, Error)]
#[cfg_attr(test, derive(PartialEq))]
pub enum TransactionValidatorError {
    #[error("Expected a positive amount of {resource:?}. Got {resource_bounds:?}.")]
    ZeroFee {
        resource: Resource,
        resource_bounds: ResourceBounds,
    },
    #[error("The resource bounds mapping is missing a resource {resource:?}.")]
    MissingResource { resource: Resource },
}

pub type TransactionValidatorResult<T> = Result<T, TransactionValidatorError>;
