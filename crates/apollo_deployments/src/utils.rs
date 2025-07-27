use std::collections::HashSet;

use apollo_protobuf::consensus::DEFAULT_VALIDATOR_ID;
use strum::IntoEnumIterator;

use crate::deployment_definitions::ServicePort;

pub(crate) fn get_validator_id(id: usize) -> String {
    format!("0x{:x}", id + usize::try_from(DEFAULT_VALIDATOR_ID).unwrap())
}

/// Validates a vector of port numbers for correct length and uniqueness.
pub(crate) fn validate_port_numbers(ports: &[u16], required_ports_num: usize) {
    let service_ports_len = ServicePort::iter().count();
    assert_eq!(
        required_ports_num, service_ports_len,
        "Mismatch between expected number of ports ({required_ports_num}) and \
         ServicePort::iter().count() ({service_ports_len})",
    );

    assert!(
        ports.len() == required_ports_num,
        "Expected vector of length {}, got {}",
        required_ports_num,
        ports.len()
    );

    let unique: HashSet<_> = ports.iter().cloned().collect();
    assert!(unique.len() == ports.len(), "Vector contains duplicate values: {ports:?}");
}
