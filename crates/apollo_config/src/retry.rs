//! A reusable configuration for retry-with-backoff mechanisms.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::dumping::{ser_param, SerializeConfig};
use crate::{ParamPath, ParamPrivacyInput, SerializedParam};

/// A configuration for the retry mechanism.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct RetryConfig {
    /// The initial waiting time in milliseconds.
    pub retry_base_millis: u64,
    /// The maximum waiting time in milliseconds.
    pub retry_max_delay_millis: u64,
    /// The maximum number of retries.
    pub max_retries: usize,
}

impl SerializeConfig for RetryConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "retry_base_millis",
                &self.retry_base_millis,
                "Base waiting time after a failed request. After that, the time increases \
                 exponentially.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "retry_max_delay_millis",
                &self.retry_max_delay_millis,
                "Max waiting time after a failed request.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_retries",
                &self.max_retries,
                "Maximum number of retries before the node stops retrying.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}
