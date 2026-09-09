use std::sync::LazyLock;

use starknet_types_core::felt::Felt;

use crate::core::ContractAddress;

/// The address of the STRK fee contract on Starknet.
const STRK_FEE_CONTRACT_ADDRESS_STR: &str =
    "0x04718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d";
/// The address of the ETH fee contract on Starknet.
const ETH_FEE_CONTRACT_ADDRESS_STR: &str =
    "0x49d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7";

/// The address of the STRK fee contract on Starknet.
pub static STRK_FEE_CONTRACT_ADDRESS: LazyLock<ContractAddress> = LazyLock::new(|| {
    ContractAddress::try_from(
        Felt::from_hex(STRK_FEE_CONTRACT_ADDRESS_STR)
            .expect("Error converting strk fee contract address from hex"),
    )
    .expect("Error converting strk fee contract address from felt")
});

/// The address of the ETH fee contract on Starknet.
pub static ETH_FEE_CONTRACT_ADDRESS: LazyLock<ContractAddress> = LazyLock::new(|| {
    ContractAddress::try_from(
        Felt::from_hex(ETH_FEE_CONTRACT_ADDRESS_STR)
            .expect("Error converting eth fee contract address from hex"),
    )
    .expect("Error converting eth fee contract address from felt")
});
