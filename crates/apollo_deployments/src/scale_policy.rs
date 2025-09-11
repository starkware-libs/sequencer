use serde::{Serialize, Serializer};

const IDLE_CONNECTIONS_FOR_AUTO_SCALED_SERVICES: usize = 0;
const IDLE_CONNECTIONS_FOR_STATICALLY_SCALED_SERVICES: usize = 10;

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

impl ScalePolicy {
    pub fn idle_connections(&self) -> usize {
        match self {
            ScalePolicy::AutoScaled => IDLE_CONNECTIONS_FOR_AUTO_SCALED_SERVICES,
            ScalePolicy::StaticallyScaled => IDLE_CONNECTIONS_FOR_STATICALLY_SCALED_SERVICES,
        }
    }
}
