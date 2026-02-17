use serde::{Serialize, Serializer};

const IDLE_CONNECTIONS_FOR_AUTO_SCALED_SERVICES: usize = 0;
const IDLE_CONNECTIONS_FOR_STATICALLY_SCALED_SERVICES: usize = 10;

// Note: we explicitly use a new connection when sending a request to an autoscaled or a
// service allowed to run on a spot instance to ensure each new request goes through the load
// balancer and directed at an available server. This allows us to avoid having to address
// connection termination issues, e.g., when the server is marked for eviction, and should be in
// a graceful shutdown flow, and as such should reject new requests.

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
