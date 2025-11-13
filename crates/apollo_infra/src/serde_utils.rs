use std::fmt::Debug;

use serde::{Deserialize, Serialize};
use serde_json::{from_slice, to_vec};

#[cfg(test)]
#[path = "serde_utils_test.rs"]
pub mod serde_utils_test;

#[cfg(test)]
#[path = "trace_util_tests.rs"]
pub mod trace_util_tests;

// A generic wrapper struct for binary serialization and deserialization, used for remote component
// communication.
#[derive(Serialize, Deserialize, Debug)]
pub struct SerdeWrapper<T> {
    data: T,
}

impl<T> SerdeWrapper<T>
where
    T: Serialize + for<'de> Deserialize<'de> + Debug,
{
    pub fn new(data: T) -> Self {
        Self { data }
    }

    pub fn wrapper_serialize(&self) -> Result<Vec<u8>, serde_json::Error> {
        to_vec(self)
    }

    pub fn wrapper_deserialize(bytes: &[u8]) -> Result<T, serde_json::Error> {
        from_slice(bytes).map(|serde_wrapper: Self| serde_wrapper.data)
    }
}
