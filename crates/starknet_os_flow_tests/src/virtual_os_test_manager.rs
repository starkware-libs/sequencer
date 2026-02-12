use starknet_api::transaction::fields::VIRTUAL_OS_OUTPUT_VERSION;
use starknet_api::transaction::MessageToL1;
use starknet_os::io::virtual_os_output::{
    compute_messages_to_l1_hashes,
    VirtualOsOutput,
    VirtualOsRunnerOutput,
};
use starknet_os::runner::run_virtual_os;

use crate::initial_state::FlowTestState;
use crate::test_manager::TestRunner;

/// The output of running the virtual OS for testing.
pub(crate) struct VirtualOsTestOutput {
    /// The raw runner output from the virtual OS.
    pub(crate) runner_output: VirtualOsRunnerOutput,
    /// The expected values computed from the OS hints.
    pub(crate) expected_virtual_os_output: VirtualOsOutput,
    /// The L2-to-L1 messages produced by the executed transactions.
    pub(crate) messages_to_l1: Vec<MessageToL1>,
    // TODO(Yoni): consider adding more data for sanity checks, such as the expected state diff.
}

impl VirtualOsTestOutput {
    /// Validates the runner output against the expected values.
    pub(crate) fn validate(&self) {
        let virtual_os_output = VirtualOsOutput::from_raw_output(&self.runner_output.raw_output)
            .expect("Parsing virtual OS output should not fail.");

        assert_eq!(virtual_os_output, self.expected_virtual_os_output);
    }
}

impl<S: FlowTestState> TestRunner<S> {
    /// Runs the virtual OS and returns the test output.
    pub(crate) fn run_virtual(self) -> VirtualOsTestOutput {
        // Create expected values before running the virtual OS (os_hints is consumed).
        let first_block = self.os_hints.os_input.os_block_inputs.first().unwrap();
        // The virtual os does not support state diff encryption.
        let config_hash =
            self.os_hints.os_hints_config.chain_info.compute_virtual_os_config_hash().unwrap();

        let messages_to_l1_hashes = compute_messages_to_l1_hashes(&self.messages_to_l1);
        let messages_to_l1 = self.messages_to_l1;
        let expected_virtual_os_output = VirtualOsOutput {
            version: VIRTUAL_OS_OUTPUT_VERSION,
            base_block_number: first_block.block_info.block_number,
            base_block_hash: first_block.new_block_hash.0,
            starknet_os_config_hash: config_hash,
            messages_to_l1_hashes,
        };

        // Run the virtual OS.
        let runner_output =
            run_virtual_os(self.os_hints).expect("Running virtual OS should not fail.");

        VirtualOsTestOutput { runner_output, expected_virtual_os_output, messages_to_l1 }
    }

    /// Runs the virtual OS and validates the output against expected values.
    pub(crate) fn run_virtual_and_validate(self) {
        self.run_virtual().validate();
    }

    /// Runs the virtual OS and expects it to fail with an error containing the given string.
    pub(crate) fn run_virtual_expect_error(self, expected_error: &str) {
        let err = run_virtual_os(self.os_hints).expect_err("Expected virtual OS to fail");
        let err_string = err.to_string();
        assert!(
            err_string.contains(expected_error),
            "Expected error to contain '{}', got: {}",
            expected_error,
            err_string
        );
    }
}
