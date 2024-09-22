use std::fmt::Debug;

use bincode::{deserialize, serialize};
use serde::{Deserialize, Serialize};

#[cfg(test)]
#[path = "serde_utils_test.rs"]
pub mod serde_utils_test;

// A generic wrapper struct for binary serialization and deserialization, used for remote component
// communication.
#[derive(Serialize, Deserialize, Debug)]
pub struct BincodeSerdeWrapper<T> {
    data: T,
}

impl<T> BincodeSerdeWrapper<T>
where
    T: Serialize + for<'de> Deserialize<'de> + Debug,
{
    pub fn new(data: T) -> Self {
        Self { data }
    }

    pub fn to_bincode(&self) -> Result<Vec<u8>, bincode::Error> {
        serialize(self)
    }

    pub fn from_bincode(bytes: &[u8]) -> Result<T, bincode::Error> {
        deserialize(bytes).map(|serde_wrapper: Self| serde_wrapper.data)
    }
}
