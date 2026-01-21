use serde::{Serialize, Serializer};

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
