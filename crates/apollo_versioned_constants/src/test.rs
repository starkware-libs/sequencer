use starknet_api::block::StarknetVersion;
use starknet_api::versioned_constants_logic::VersionedConstantsTrait;
use strum::IntoEnumIterator;

use super::VersionedConstants;

#[test]
fn max_gas_price_multiplier_is_above_one_in_every_version() {
    let supported_constants: Vec<_> = StarknetVersion::iter()
        .filter_map(|version| {
            VersionedConstants::get(&version)
                .ok()
                .map(|versioned_constants| (version, versioned_constants))
        })
        .collect();
    assert!(!supported_constants.is_empty());
    for (version, versioned_constants) in supported_constants {
        assert!(
            versioned_constants.max_gas_price_multiplier > 1,
            "max_gas_price_multiplier of {version} must be greater than one, got {}.",
            versioned_constants.max_gas_price_multiplier
        );
    }
}
