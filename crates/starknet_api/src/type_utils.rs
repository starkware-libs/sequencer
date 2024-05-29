/// Implements `From<bottom_type> for top_type` for all bottom_types. Assumes:
/// - `From<intermediate_type> for top_type` is implemented.
/// - `From<bottom_type> for intermediate_type` is implemented, for all bottom_types.
#[macro_export]
macro_rules! impl_from_through_intermediate {
    ($intermediate_type: ty, $top_type:ty, $($bottom_type:ty),+) => {
        $(
            impl From<$bottom_type> for $top_type {
                fn from(x: $bottom_type) -> Self {
                    Self::from(<$intermediate_type>::from(x))
                }
            }
        )+
    };
}
