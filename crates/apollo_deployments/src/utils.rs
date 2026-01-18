use std::collections::HashSet;

use apollo_node_config::component_execution_config::DEFAULT_INVALID_PORT;
use static_assertions::const_assert_ne;

const INFRA_PORT_PLACEHOLDER: u16 = 1;
const_assert_ne!(INFRA_PORT_PLACEHOLDER, DEFAULT_INVALID_PORT);

/// A generator-like for setting infra ports in different services.
/// - If constructed with `Some(vec)`: yields values from the vec, up to `expected_len` times.
/// - If constructed with `None`: yields `INFRA_PORT_PLACEHOLDER`, up to `expected_len` times.
/// - On `Drop`: asserts it has been fully depleted (all `expected_len` values were yielded).
pub(crate) struct InfraPortAllocator {
    expected_len: usize,
    idx: usize,
    values: Vec<u16>,
}

impl InfraPortAllocator {
    pub fn new(values: Option<Vec<u16>>, expected_len: usize) -> Self {
        let values = match values {
            Some(v) => {
                validate_ports(&v, expected_len);
                v
            }
            None => vec![INFRA_PORT_PLACEHOLDER; expected_len],
        };

        Self { expected_len, idx: 0, values }
    }

    /// Returns the next value. Panics if called more than `expected_len` times.
    pub fn next(&mut self) -> u16 {
        assert!(
            self.idx < self.expected_len,
            "InfraPortAllocator exhausted: expected_len is {}",
            self.expected_len
        );
        let out = self.values[self.idx];
        self.idx += 1;
        out
    }
}

impl Drop for InfraPortAllocator {
    fn drop(&mut self) {
        assert!(
            self.idx == self.expected_len,
            "InfraPortAllocator dropped before being depleted: produced {} out of {} values",
            self.idx,
            self.expected_len
        );
    }
}

// Validates that the provided ports vector has the correct length and all unique values.
fn validate_ports(ports: &[u16], required_ports_num: usize) {
    let ports_len = ports.len();
    assert_eq!(
        ports_len, required_ports_num,
        "Expected vector of length {required_ports_num}, got {ports_len}",
    );

    let unique: HashSet<_> = ports.iter().cloned().collect();
    assert_eq!(unique.len(), ports_len, "Vector contains duplicate values: {ports:?}");
}
