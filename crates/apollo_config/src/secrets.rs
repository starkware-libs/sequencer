//! Utils for serializing config objects into flatten map and json file.
//! The elements structure is:

use core::fmt;
use serde::{Serialize, Deserialize, Serializer};

#[repr(transparent)]
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
pub struct Sensitive<T>(pub T);

impl<T> fmt::Debug for Sensitive<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str("«redacted»") }
}
impl<T> fmt::Display for Sensitive<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str("«redacted»") }
}

impl<T> Sensitive<T> {
    pub fn new(inner: T) -> Self { Self(inner) }
    pub fn expose(&self) -> &T { &self.0 }
}

// Serialize as a literal string "«redacted»"
impl<T> Serialize for Sensitive<T> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str("«redacted»")
    }
}
