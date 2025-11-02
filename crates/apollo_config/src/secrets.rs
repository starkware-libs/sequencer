use core::fmt;

use serde::{Deserialize, Serialize, Serializer};

const REDACTED: &str = "<<redacted>>";

#[derive(Clone, Deserialize)]
#[serde(transparent, bound(deserialize = "T: Deserialize<'de>"))]
pub struct Sensitive<T> {
    inner: T,
    #[serde(skip)]
    redactor: Option<fn(&T) -> String>,
}

impl<T> Sensitive<T> {
    pub fn new(inner: T, redactor: Option<fn(&T) -> String>) -> Self {
        Self { inner, redactor }
    }

    pub fn into(self) -> T {
        self.inner
    }

    fn redact_str(&self) -> String {
        match self.redactor {
            Some(f) => f(&self.inner),
            None => REDACTED.to_string(),
        }
    }
}

// Equality/ordering/hash only consider the inner value (ignore redactor)
impl<T: PartialEq> PartialEq for Sensitive<T> {
    fn eq(&self, other: &Self) -> bool {
        self.inner.eq(&other.inner)
    }
}
impl<T: Eq> Eq for Sensitive<T> {}
impl<T: PartialOrd> PartialOrd for Sensitive<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.inner.partial_cmp(&other.inner)
    }
}
impl<T: Ord> Ord for Sensitive<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.inner.cmp(&other.inner)
    }
}
impl<T: std::hash::Hash> std::hash::Hash for Sensitive<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.inner.hash(state)
    }
}

impl<T> fmt::Debug for Sensitive<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.redact_str())
    }
}
impl<T> fmt::Display for Sensitive<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.redact_str())
    }
}

impl<T> Serialize for Sensitive<T> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.redact_str())
    }
}
