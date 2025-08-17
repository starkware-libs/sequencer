pub trait LabeledRequest {
    type Disc: Send + Sync + 'static + Into<&'static str>;
    fn disc(&self) -> Self::Disc;
}

pub const LABEL_NAME_REQUEST_VARIANT: &str = "request_variant";

#[macro_export]
macro_rules! impl_labeled_request {
    ($Enum:ty, $Disc:ty) => {
        impl $crate::requests::LabeledRequest for $Enum {
            type Disc = $Disc;
            fn disc(&self) -> Self::Disc {
                <$Disc>::from(self)
            }
        }
    };
}
