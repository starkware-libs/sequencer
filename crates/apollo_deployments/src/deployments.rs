pub mod consolidated;
pub mod distributed;
pub mod hybrid;

pub(crate) const IDLE_CONNECTIONS_FOR_AUTOSCALED_SERVICES: usize = 0;
pub(crate) const RETRIES_FOR_L1_SERVICES: usize = 2;
