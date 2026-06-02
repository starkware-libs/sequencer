use std::io;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum FeederGatewayRunError {
    #[error(transparent)]
    ServerStartupError(#[from] io::Error),
}
