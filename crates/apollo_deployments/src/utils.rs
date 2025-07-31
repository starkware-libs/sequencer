use std::collections::HashSet;

use apollo_protobuf::consensus::DEFAULT_VALIDATOR_ID;

use crate::deployment_definitions::ServicePort;

pub(crate) fn get_validator_id(id: usize) -> String {
    format!("0x{:x}", id + usize::try_from(DEFAULT_VALIDATOR_ID).unwrap())
}

/// Validates a vector of port numbers for correct length and uniqueness.
///
/// The `ServicePort::iter().count()` represents the number of defined service ports
/// in the `ServicePort` enum. This is compared against the required number of ports
/// passed into the function.
pub(crate) fn validate_port_numbers(ports: &[u16], required_ports_num: usize) {
    let service_ports_len = ServicePort::iter().count();
    let ports_num = ports.len();
    assert_eq!(
        required_ports_num, service_ports_len,
        "Mismatch between expected number of ports ({required_ports_num}) and \
         ServicePort::iter().count() ({service_ports_len})",
    );

    assert!(
        ports_num == required_ports_num,
        "Expected vector of length {}, got {}",
        required_ports_num,
        ports_num
    );

    let unique: HashSet<_> = ports.iter().cloned().collect();
    assert!(unique.len() == ports_num, "Vector contains duplicate values: {ports:?}");
}
