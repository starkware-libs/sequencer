use blockifier::blockifier_versioned_constants::VersionedConstants;

use crate::config::DEFAULT_MAX_L2_GAS_AMOUNT;

#[test]
fn test_default_max_l2_gas_amount() {
    let versioned_constants = VersionedConstants::latest_constants();

    let validate_max_sierra_gas = versioned_constants.os_constants.validate_max_sierra_gas;
    let execute_max_sierra_gas = versioned_constants.os_constants.execute_max_sierra_gas;

    let max_l2_gas_amount = validate_max_sierra_gas.0 + execute_max_sierra_gas.0;

    assert_eq!(max_l2_gas_amount, DEFAULT_MAX_L2_GAS_AMOUNT);
}
