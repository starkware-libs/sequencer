use std::collections::HashMap;

use apollo_starknet_os_program::test_programs::ALIASES_TEST_BYTES;

use crate::test_utils::cairo_runner::EntryPointRunnerConfig;
use crate::test_utils::utils::test_cairo_function;
// TODO(Nimrod): Move this next to the stateful compression hints implementation.
// TODO(Amos): This test is incomplete. Add the rest of the test cases and remove this todo.

#[test]
fn test_constants() {
    let max_non_compressed_contract_address = 15;
    let alias_counter_storage_key = 0;
    let initial_available_alias = 128;
    let alias_contract_address = 2;
    test_cairo_function(
        &EntryPointRunnerConfig::default(),
        ALIASES_TEST_BYTES,
        "test_constants",
        &[
            max_non_compressed_contract_address.into(),
            alias_counter_storage_key.into(),
            initial_available_alias.into(),
            alias_contract_address.into(),
        ],
        &[],
        &[],
        &[],
        HashMap::new(),
    )
}
