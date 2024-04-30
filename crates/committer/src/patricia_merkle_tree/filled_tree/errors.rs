#[derive(thiserror::Error, Debug, derive_more::Display)]
pub(crate) enum FilledTreeError {
    MissingRoot,
    SerializeError(#[from] serde_json::Error),
}
