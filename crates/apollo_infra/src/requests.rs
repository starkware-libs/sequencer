pub trait LabeledRequest {
    fn request_label(&self) -> &'static str;
}

pub const LABEL_NAME_REQUEST_VARIANT: &str = "request_variant";

#[macro_export]
macro_rules! impl_labeled_request {
    ($Enum:ty, $RequestLabel:ty) => {
        impl $crate::requests::LabeledRequest for $Enum {
            fn request_label(&self) -> &'static str {
                let label: $RequestLabel = <$RequestLabel>::from(self);
                label.into()
            }
        }
    };
}
