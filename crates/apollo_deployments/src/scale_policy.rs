use serde::{Serialize, Serializer};

pub(crate) const IDLE_CONNECTIONS_FOR_AUTOSCALED_SERVICES: usize = 0;

/// Whether a service is autoscaled or not.
#[derive(Clone, Debug, PartialEq)]
pub enum ScalePolicy {
    // The service is autoscaled.
    AutoScaled,
    // The service is not autoscaled.
    StaticallyScaled,
}

impl Serialize for ScalePolicy {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            ScalePolicy::AutoScaled => serializer.serialize_bool(true),
            ScalePolicy::StaticallyScaled => serializer.serialize_bool(false),
        }
    }
}
