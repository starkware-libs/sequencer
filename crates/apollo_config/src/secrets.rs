//! A wrapper for values that are considered **sensitive** (e.g. secrets, tokens, URLs).
//!
//! `Sensitive<T>` keeps the inner value available while preventing accidental leakage through
//!  formatting, logging, and serialization:
//!
//! - Display/Debug/Serialize: returns a redacted default value, or a custom redaction via the
//!   provided `redactor`.
//! - Deserialize: transparent, and deserializes exactly like `T`, ignoring the `redactor` field.

use core::fmt;

use serde::{Deserialize, Serialize, Serializer};

#[cfg(test)]
#[path = "secrets_test.rs"]
mod secrets_test;

const DEFAULT_REDACTION_OUTPUT: &str = "<<redacted>>";

type Redactor<T> = Box<dyn Fn(&T) -> String + Send + Sync + 'static>;

#[derive(Deserialize)]
#[serde(transparent, bound(deserialize = "T: Deserialize<'de>"))]
pub struct Sensitive<T> {
    inner: T,
    #[serde(skip)]
    redactor: Option<Redactor<T>>,
}

impl<T> Sensitive<T> {
    /// Creates a new `Sensitive<T>` with no custom redactor.
    pub fn new(inner: T) -> Self {
        Self { inner, redactor: None }
    }

    /// Attaches a custom redactor function to this `Sensitive` value.
    pub fn with_redactor<F>(mut self, redactor: F) -> Self
    where
        F: Fn(&T) -> String + Send + Sync + 'static,
    {
        self.redactor = Some(Box::new(redactor));
        self
    }

    /// Consumes the wrapper and returns the inner sensitive value.
    pub fn into(self) -> T {
        self.inner
    }

    // Returns the redacted string representation.
    fn redact(&self) -> String {
        match &self.redactor {
            Some(f) => f(&self.inner),
            None => DEFAULT_REDACTION_OUTPUT.to_string(),
        }
    }
}

impl<T> AsRef<T> for Sensitive<T> {
    fn as_ref(&self) -> &T {
        &self.inner
    }
}

impl<T> AsMut<T> for Sensitive<T> {
    fn as_mut(&mut self) -> &mut T {
        &mut self.inner
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
        f.write_str(&self.redact())
    }
}
impl<T> fmt::Display for Sensitive<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.redact())
    }
}
impl<T> Serialize for Sensitive<T> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.redact())
    }
}
