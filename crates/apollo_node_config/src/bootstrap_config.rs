use std::collections::BTreeMap;

use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_api::core::ContractAddress;
use validator::Validate;

/// Configuration for the bootstrap mode.
///
/// Bootstrap mode allows the node to start with empty storage and automatically
/// execute hardcoded bootstrap transactions (declare contracts, deploy accounts,
/// deploy fee tokens, etc.) without validation.
///
/// The node will exit bootstrap mode when the funded account has sufficient balance
/// in both ETH and STRK ERC20 tokens.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Validate)]
pub struct BootstrapConfig {
    /// When true, the node will automatically enter bootstrap mode if storage is empty.
    /// Bootstrap transactions will be executed without validation.
    pub enable_bootstrap_mode: bool,

    /// The address of the account that will be funded during bootstrap.
    /// This address is deterministically calculated from the account contract class hash
    /// and deployment salt.
    /// Bootstrap mode will exit when this account has sufficient balance.
    pub funded_account_address: ContractAddress,

    /// The minimum balance (in both ETH and STRK) required to exit bootstrap mode.
    /// Once the funded account has at least this balance in both fee tokens,
    /// bootstrap mode will be considered complete.
    pub required_balance: u128,
}

impl Default for BootstrapConfig {
    fn default() -> Self {
        Self {
            enable_bootstrap_mode: false,
            funded_account_address: ContractAddress::default(),
            required_balance: 0,
        }
    }
}

impl SerializeConfig for BootstrapConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([
            ser_param(
                "enable_bootstrap_mode",
                &self.enable_bootstrap_mode,
                "When true, the node will automatically enter bootstrap mode if storage is empty.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "funded_account_address",
                &self.funded_account_address,
                "The address of the account that will be funded during bootstrap. Bootstrap mode \
                 will exit when this account has sufficient balance.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "required_balance",
                &self.required_balance,
                "The minimum balance (in both ETH and STRK) required to exit bootstrap mode.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}
