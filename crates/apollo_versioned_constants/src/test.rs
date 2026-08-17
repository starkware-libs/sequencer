use starknet_api::block::StarknetVersion;
use starknet_api::versioned_constants_logic::VersionedConstantsTrait;
use strum::IntoEnumIterator;

use crate::VersionedConstants;

// A multiplier of 0 or 1 pins the L2 gas price at 0 or at the minimum.
#[test]
fn max_gas_price_multiplier_leaves_room_above_the_minimum() {
    let first_version = VersionedConstants::first_version();
    for version in StarknetVersion::iter() {
        let Ok(constants) = VersionedConstants::get(&version) else {
            // Only versions below the first one with versioned constants may lack a JSON entry.
            assert!(version < first_version, "Version {version} has no versioned constants.");
            continue;
        };
        assert!(
            constants.max_gas_price_multiplier > 1,
            "Version {version} has max_gas_price_multiplier {}, which must be greater than one.",
            constants.max_gas_price_multiplier
        );
    }
}
