use std::collections::HashSet;

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
