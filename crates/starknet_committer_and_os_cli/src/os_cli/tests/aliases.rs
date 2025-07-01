use std::collections::HashMap;

use apollo_starknet_os_program::test_programs::ALIASES_TEST_BYTES;
use starknet_os::test_utils::cairo_runner::EntryPointRunnerConfig;
use tracing::info;

use crate::os_cli::tests::types::OsPythonTestResult;
use crate::os_cli::tests::utils::test_cairo_function;

// TODO(Amos): This test is incomplete. Add the rest of the test cases and remove this todo.
#[allow(clippy::result_large_err)]
pub(crate) fn aliases_test() -> OsPythonTestResult {
    info!("Testing `test_constants`...");
    test_constants()?;
    Ok("".to_string())
}

#[allow(clippy::result_large_err)]
fn test_constants() -> OsPythonTestResult {
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
