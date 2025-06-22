use std::collections::HashSet;

use crate::deployment::DeploymentType;

// TODO(Tsabary): test no conflicts between config entries defined in each of the override types.
// TODO(Tsabary): delete duplicates from the base app config, and add a test that there are no
// conflicts between all the override config entries and the values in the base app config.

pub(crate) fn format_node_id(base_format: &str, id: usize) -> String {
    base_format.replace("{}", &id.to_string())
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
