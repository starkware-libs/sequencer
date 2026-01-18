use serde::{Serialize, Serializer};

use crate::alerts::AlertSeverity;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ComparisonValueOrPlaceholder {
    ConcreteValue(f64),
    Placeholder(String),
}

impl From<f64> for ComparisonValueOrPlaceholder {
    fn from(value: f64) -> Self {
        ComparisonValueOrPlaceholder::ConcreteValue(value)
    }
}

impl From<String> for ComparisonValueOrPlaceholder {
    fn from(value: String) -> Self {
        ComparisonValueOrPlaceholder::Placeholder(value)
    }
}

impl Serialize for ComparisonValueOrPlaceholder {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            ComparisonValueOrPlaceholder::ConcreteValue(value) => value.serialize(serializer),
            ComparisonValueOrPlaceholder::Placeholder(placeholder) => {
                placeholder.serialize(serializer)
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SeverityValueOrPlaceholder {
    ConcreteValue(AlertSeverity),
    Placeholder(String),
}

impl From<AlertSeverity> for SeverityValueOrPlaceholder {
    fn from(value: AlertSeverity) -> Self {
        SeverityValueOrPlaceholder::ConcreteValue(value)
    }
}

impl From<String> for SeverityValueOrPlaceholder {
    fn from(value: String) -> Self {
        SeverityValueOrPlaceholder::Placeholder(value)
    }
}

impl Serialize for SeverityValueOrPlaceholder {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            SeverityValueOrPlaceholder::ConcreteValue(severity) => severity.serialize(serializer),
            SeverityValueOrPlaceholder::Placeholder(placeholder) => {
                placeholder.serialize(serializer)
            }
        }
    }
}
