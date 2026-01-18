use apollo_infra_utils::template::Template;
use serde::{Serialize, Serializer};

use crate::alerts::AlertSeverity;

const ALERT_PLACEHOLDER_FORMAT: &str = "$$$_{}_$$$";

fn format_alert_placeholder(key: &String) -> String {
    Template::new(ALERT_PLACEHOLDER_FORMAT).format(&[&key]).to_uppercase()
}

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
                format_alert_placeholder(placeholder).serialize(serializer)
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
                format_alert_placeholder(placeholder).serialize(serializer)
            }
        }
    }
}

// TODO(Tsabary): remove the `Clone` and `PartialEq` constraints.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ExpressionOrExpressionWithPlaceholder {
    ConcreteValue(String),
    Placeholder(Template, Vec<String>),
}

impl From<String> for ExpressionOrExpressionWithPlaceholder {
    fn from(value: String) -> Self {
        ExpressionOrExpressionWithPlaceholder::ConcreteValue(value)
    }
}

impl From<(Template, String)> for ExpressionOrExpressionWithPlaceholder {
    fn from((template, placeholder): (Template, String)) -> Self {
        ExpressionOrExpressionWithPlaceholder::Placeholder(template, vec![placeholder])
    }
}

impl Serialize for ExpressionOrExpressionWithPlaceholder {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            ExpressionOrExpressionWithPlaceholder::ConcreteValue(expression) => {
                expression.serialize(serializer)
            }
            ExpressionOrExpressionWithPlaceholder::Placeholder(template, placeholders) => {
                format_alert_placeholder(&template.format(placeholders.as_slice()))
                    .serialize(serializer)
            }
        }
    }
}
