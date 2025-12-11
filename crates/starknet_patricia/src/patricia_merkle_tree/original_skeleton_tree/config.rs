/// Configures the creation of an original skeleton tree.
pub trait OriginalSkeletonTreeConfig {
    /// Configures whether modified leaves should be compared to the previous leaves and log out a
    /// warning when encountering a trivial modification.
    fn compare_modified_leaves(&self) -> bool;
}
