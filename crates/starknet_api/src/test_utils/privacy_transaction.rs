//! Constructs a realistic client-side proving InvokeTransactionV3 for cairo.pie generation.
//!
//! Used to provide representative client-side proving traces for prover optimization.

use super::invoke::{invoke_tx, InvokeTxArgs};
use crate::block::GasPrice;
use crate::core::{ContractAddress, Nonce};
use crate::execution_resources::GasAmount;
use crate::transaction::fields::{AllResourceBounds, ResourceBounds, ValidResourceBounds};
use crate::transaction::InvokeTransaction;
use crate::{calldata, contract_address, felt};

/// Raw hex constants defining a real privacy pool InvokeTransactionV3.
///
/// Constants were copied from the `Signed Transaction (JSON)` output of:
/// `bazel run //src/starkware/starknet/services/single_tx_runner:single_tx_runner -- \
///   --config config_privacy_export.yml --export-only`.
///
/// Starting context:
/// - Account address: 0x06aD5754Abe954c193CeE3D9B15Ac84e4AC562dFac6287E2b99D56bB5e10adcb
/// - Token: STRK
/// - note0: 60 STRK, randomness = 0x7df2e0febf7b49789620f89f79ff5
/// - note1: 40 STRK, randomness = 0x1a3fc3168f27d39a708ad8c2d44d9c
mod constants {
    /// Sender is the privacy pool contract.
    pub const SENDER_ADDRESS: &str =
        "0x712391ff6487c9232582442ea7eb4a10cad4892c3bcde3516e2a3955bf4f0da";
}

/// Creates a privacy pool invoke transaction for testing.
///
/// Uses the same pattern as `strk_balance_of_invoke` in runner_test.rs.
pub fn create_privacy_invoke_tx() -> InvokeTransaction {
    let sender_address: ContractAddress = contract_address!(constants::SENDER_ADDRESS);

    pub const NONCE: &str = "0x7";
    // Privacy pool calldata - calls the privacy pool contract.
    // Calldata semantics:
    // - Consumes note0 (60 STRK) and note1 (40 STRK).
    // - Creates:
    // - note2: 90 STRK, randomness = 0xe08b0a271b4e1d1030f5f89ca0dbc8
    // - note3: 10 STRK, randomness = 0xa167508bf91d497f245c6e1cf4e110
    let calldata = calldata![
        felt!("0x6ad5754abe954c193cee3d9b15ac84e4ac562dfac6287e2b99d56bb5e10adcb"),
        felt!("0x4"),
        felt!("0x5"),
        felt!("0x9874a02fe5bbda5d097a608675f2a5a71e2ea38b4438c51e90d8084a1e88e1"),
        felt!("0x3aab600ef074da54eaec6c828131ac970c62335d99f89da6dfe18eb55a7b648"),
        felt!("0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d"),
        felt!("0x0"),
        felt!("0x5"),
        felt!("0x9874a02fe5bbda5d097a608675f2a5a71e2ea38b4438c51e90d8084a1e88e1"),
        felt!("0x3aab600ef074da54eaec6c828131ac970c62335d99f89da6dfe18eb55a7b648"),
        felt!("0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d"),
        felt!("0x1"),
        felt!("0x3"),
        felt!("0x9874a02fe5bbda5d097a608675f2a5a71e2ea38b4438c51e90d8084a1e88e1"),
        felt!("0x6ad5754abe954c193cee3d9b15ac84e4ac562dfac6287e2b99d56bb5e10adcb"),
        felt!("0xfefe558519ee1cf0a1f6999eaa3d35d01ecb880badc6618fe26342fbee59aa"),
        felt!("0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d"),
        felt!("0x4e1003b28d9280000"),
        felt!("0x2"),
        felt!("0xe08b0a271b4e1d1030f5f89ca0dbc8"),
        felt!("0x3"),
        felt!("0x9874a02fe5bbda5d097a608675f2a5a71e2ea38b4438c51e90d8084a1e88e1"),
        felt!("0x6ad5754abe954c193cee3d9b15ac84e4ac562dfac6287e2b99d56bb5e10adcb"),
        felt!("0xfefe558519ee1cf0a1f6999eaa3d35d01ecb880badc6618fe26342fbee59aa"),
        felt!("0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d"),
        felt!("0x8ac7230489e80000"),
        felt!("0x3"),
        felt!("0xa167508bf91d497f245c6e1cf4e110")
    ];

    let resource_bounds = ValidResourceBounds::AllResources(AllResourceBounds {
        l1_gas: ResourceBounds { max_amount: GasAmount(0), max_price_per_unit: GasPrice(0) },
        l2_gas: ResourceBounds {
            max_amount: GasAmount(10_000_000),
            max_price_per_unit: GasPrice(0),
        },
        l1_data_gas: ResourceBounds { max_amount: GasAmount(0), max_price_per_unit: GasPrice(0) },
    });

    invoke_tx(InvokeTxArgs {
        sender_address,
        calldata,
        resource_bounds,
        nonce: Nonce(felt!(NONCE)),
        ..Default::default()
    })
}
