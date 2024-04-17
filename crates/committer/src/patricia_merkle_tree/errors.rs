// TODO(Amos, 01/04/2024): Add error types.
#[derive(Debug)]
pub(crate) enum OriginalSkeletonTreeError {}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) enum UpdatedSkeletonTreeError {
    MissingNode,
    PoisonedLock(String),
    NonDroppedPointer(String),
}

#[derive(Debug)]
pub(crate) enum FilledTreeError {
    MissingRoot,
}
