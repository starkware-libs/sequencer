use std::collections::HashSet;
use std::fmt::Display;

use crate::deployment::DeploymentType;

// TODO(Tsabary): test no conflicts between config entries defined in each of the override types.
// TODO(Tsabary): delete duplicates from the base app config, and add a test that there are no
// conflicts between all the override config entries and the values in the base app config.

/// A simple positional template with `{}` placeholders.
pub struct Template(pub &'static str);

impl Template {
    /// Renders the template by substituting `{}` placeholders with the provided args. Panics if the
    /// number of `{}` in the template doesn't match the number of args provided.
    pub fn format(&self, args: &[&dyn Display]) -> String {
        let mut formatted = self.0.to_string();

        // Count how many `{}` placeholders are in the template string.
        let placeholder_count = formatted.matches("{}").count();

        // Ensure the number of args matches the number of placeholders.
        assert!(
            placeholder_count == args.len(),
            "Template {} expects {} placeholders, but got {} args",
            self.0,
            placeholder_count,
            args.len()
        );

        // Replace each `{}` in order with the corresponding value.
        for value in args {
            // Find the index of the next `{}`.
            if let Some(i) = formatted.find("{}") {
                // Split the string into prefix and suffix, excluding the `{}` itself.
                let before = &formatted[..i];
                let after = &formatted[i + 2..];

                // Replace `{}` with the actual value.
                formatted = format!("{}{}{}", before, value, after);
            }
        }

        formatted
    }
}

pub(crate) fn get_secret_key(id: usize) -> String {
    format!("0x010101010101010101010101010101010101010101010101010101010101010{}", id + 1)
}

pub(crate) fn get_validator_id(id: usize, deployment_type: DeploymentType) -> String {
    format!("0x{:x}", id + deployment_type.validator_id_offset())
}

/// Returns a validated or generated vector of port numbers of length `n`.
/// If `ports` is `Some`, asserts it has length `n` and all unique values.
/// If `None`, generates a sequence of `n` values starting from `start`.
pub(crate) fn determine_port_numbers(
    ports: Option<Vec<u16>>,
    required_ports_num: usize,
    base_port_for_generation: u16,
) -> Vec<u16> {
    match ports {
        Some(v) => {
            assert!(
                v.len() == required_ports_num,
                "Expected vector of length {}, got {}",
                required_ports_num,
                v.len()
            );

            let unique: HashSet<_> = v.iter().cloned().collect();
            assert!(unique.len() == v.len(), "Vector contains duplicate values: {:?}", v);

            v
        }
        None => (base_port_for_generation..).take(required_ports_num).collect(),
    }
}
