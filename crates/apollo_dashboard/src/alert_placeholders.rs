use apollo_infra_utils::template::Template;
use serde::{Serialize, Serializer};

use crate::alerts::AlertSeverity;

const ALERT_PLACEHOLDER_FORMAT: &str = "$$${}-{}$$$";
const SEVERITY_CONTEXT: &str = "severity";
const COMPARISON_CONTEXT: &str = "comparison_value";
const EXPRESSION_CONTEXT: &str = "expression";

fn format_alert_placeholder(key: &String, context: &String) -> String {
    Template::new(ALERT_PLACEHOLDER_FORMAT).format(&[&key, &context])
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
                format_alert_placeholder(placeholder, &COMPARISON_CONTEXT.to_string())
                    .serialize(serializer)
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
                format_alert_placeholder(placeholder, &SEVERITY_CONTEXT.to_string())
                    .serialize(serializer)
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

impl From<&str> for ExpressionOrExpressionWithPlaceholder {
    fn from(value: &str) -> Self {
        ExpressionOrExpressionWithPlaceholder::ConcreteValue(value.to_string())
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
        let serialization = match self {
            ExpressionOrExpressionWithPlaceholder::ConcreteValue(expression) => {
                expression.to_string()
            }
            ExpressionOrExpressionWithPlaceholder::Placeholder(
                expression_template,
                placeholders,
            ) => {
                let formatted_placeholders = placeholders
                    .iter()
                    .map(|placeholder| {
                        format_alert_placeholder(placeholder, &EXPRESSION_CONTEXT.to_string())
                    })
                    .collect::<Vec<String>>();
                expression_template.format(&formatted_placeholders)
            }
        };
        // Grafana's alert evaluation does not substitute `$pod`. If we keep
        // `pod=~"$pod"` in alert PromQL, rules may evaluate to empty/no-data and stop firing.
        // TODO(Tsabary): set the pod string as a const and use it when generating the filtering
        // to begin with.
        serialization.replace(", pod=~\"$pod\"", "").serialize(serializer)
    }
}
