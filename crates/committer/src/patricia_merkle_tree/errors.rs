// TODO(Amos, 01/04/2024): Add error types.
#[derive(Debug)]
pub(crate) enum OriginalSkeletonTreeError {}

#[derive(Debug)]
pub(crate) enum UpdatedSkeletonTreeError {
    MissingNode,
}

#[derive(Debug)]
pub(crate) enum FilledTreeError {
    MissingRoot,
}
