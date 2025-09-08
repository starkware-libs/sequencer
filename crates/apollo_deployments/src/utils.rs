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

<<<<<<< HEAD
            let unique: HashSet<_> = v.iter().cloned().collect();
            assert!(unique.len() == v.len(), "Vector contains duplicate values: {v:?}");

            v
        }
        None => (base_port_for_generation..).take(required_ports_num).collect(),
    }
=======
    let unique: HashSet<_> = ports.iter().cloned().collect();
    assert_eq!(unique.len(), ports_len, "Vector contains duplicate values: {ports:?}");
>>>>>>> origin/main-v0.14.0
}
