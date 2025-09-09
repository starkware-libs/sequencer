use std::collections::HashSet;

use apollo_protobuf::consensus::DEFAULT_VALIDATOR_ID;

pub(crate) fn get_validator_id(id: usize) -> String {
    format!("0x{:x}", id + usize::try_from(DEFAULT_VALIDATOR_ID).unwrap())
}

/// Validates that the provided ports vector has the correct length and all unique values.
pub(crate) fn validate_ports(ports: &[u16], required_ports_num: usize) {
    let ports_len = ports.len();
    assert_eq!(
        ports_len, required_ports_num,
        "Expected vector of length {required_ports_num}, got {ports_len}",
    );

    let unique: HashSet<_> = ports.iter().cloned().collect();
    assert_eq!(unique.len(), ports_len, "Vector contains duplicate values: {ports:?}");
}
