use thiserror::Error;

#[derive(Error, Debug)]
pub enum CommitmentManagerError {
    #[error("No commitment results available in the results channel.")]
    ResultsChannelEmpty,
}
