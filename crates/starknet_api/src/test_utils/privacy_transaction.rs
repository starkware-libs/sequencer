//! Constructs a realistic client-side proving InvokeTransactionV3 for cairo.pie generation.
//!
//! Used to provide representative client-side proving traces for prover optimization.

use std::sync::Arc;

use num_traits::Num;

use crate::block::GasPrice;
use crate::core::Nonce;
use crate::data_availability::DataAvailabilityMode;
use crate::execution_resources::GasAmount;
use crate::transaction::fields::{
    AccountDeploymentData,
    AllResourceBounds,
    Calldata,
    PaymasterData,
    ProofFacts,
    ResourceBounds,
    Tip,
    TransactionSignature,
    ValidResourceBounds,
};
use crate::transaction::InvokeTransactionV3;
use crate::{contract_address, felt};

// Generic helper function for parsing hex strings to integer types
fn parse_hex<T: Num>(hex: &str) -> T
where
    <T as Num>::FromStrRadixErr: std::fmt::Debug,
{
    T::from_str_radix(hex.trim_start_matches("0x"), 16).unwrap()
}

fn parse_resource_bounds(max_amount_hex: &str, max_price_hex: &str) -> ResourceBounds {
    ResourceBounds {
        max_amount: GasAmount(parse_hex(max_amount_hex)),
        max_price_per_unit: GasPrice(parse_hex(max_price_hex)),
    }
}

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

    pub const SIGNATURE_R: &str =
        "0x51b5ccfc0b94c75e063c48a0a24fcfdaf148c4a86218293a2d6d26170400113";
    pub const SIGNATURE_S: &str =
        "0x72d94d6dd706745ee1938db5639fe7bfafb2dc8407056ddd82c64df7bfcdd60";

    pub const NONCE: &str = "0x7";
    pub const TIP: &str = "0x0";

    // Resource bounds
    pub const L1_GAS_MAX_AMOUNT: &str = "0x0";
    pub const L1_GAS_MAX_PRICE: &str = "0x0";
    pub const L2_GAS_MAX_AMOUNT: &str = "0x989680";
    pub const L2_GAS_MAX_PRICE: &str = "0x0";
    pub const L1_DATA_GAS_MAX_AMOUNT: &str = "0x0";
    pub const L1_DATA_GAS_MAX_PRICE: &str = "0x0";

    /// Calldata semantics:
    /// - Consumes note0 (60 STRK) and note1 (40 STRK).
    /// - Creates:
    ///   - note2: 90 STRK, randomness = 0xe08b0a271b4e1d1030f5f89ca0dbc8
    ///   - note3: 10 STRK, randomness = 0xa167508bf91d497f245c6e1cf4e110
    pub const CALLDATA: [&str; 28] = [
        "0x6ad5754abe954c193cee3d9b15ac84e4ac562dfac6287e2b99d56bb5e10adcb",
        "0x4",
        "0x5",
        "0x9874a02fe5bbda5d097a608675f2a5a71e2ea38b4438c51e90d8084a1e88e1",
        "0x3aab600ef074da54eaec6c828131ac970c62335d99f89da6dfe18eb55a7b648",
        "0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d",
        "0x0",
        "0x5",
        "0x9874a02fe5bbda5d097a608675f2a5a71e2ea38b4438c51e90d8084a1e88e1",
        "0x3aab600ef074da54eaec6c828131ac970c62335d99f89da6dfe18eb55a7b648",
        "0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d",
        "0x1",
        "0x3",
        "0x9874a02fe5bbda5d097a608675f2a5a71e2ea38b4438c51e90d8084a1e88e1",
        "0x6ad5754abe954c193cee3d9b15ac84e4ac562dfac6287e2b99d56bb5e10adcb",
        "0xfefe558519ee1cf0a1f6999eaa3d35d01ecb880badc6618fe26342fbee59aa",
        "0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d",
        "0x4e1003b28d9280000",
        "0x2",
        "0xe08b0a271b4e1d1030f5f89ca0dbc8",
        "0x3",
        "0x9874a02fe5bbda5d097a608675f2a5a71e2ea38b4438c51e90d8084a1e88e1",
        "0x6ad5754abe954c193cee3d9b15ac84e4ac562dfac6287e2b99d56bb5e10adcb",
        "0xfefe558519ee1cf0a1f6999eaa3d35d01ecb880badc6618fe26342fbee59aa",
        "0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d",
        "0x8ac7230489e80000",
        "0x3",
        "0xa167508bf91d497f245c6e1cf4e110",
    ];
}

/// Creates a pre-signed InvokeTransactionV3 for privacy pool testing.
pub fn create_signed_invoke_v3() -> InvokeTransactionV3 {
    InvokeTransactionV3 {
        sender_address: contract_address!(constants::SENDER_ADDRESS),
        signature: TransactionSignature(Arc::new(vec![
            felt!(constants::SIGNATURE_R),
            felt!(constants::SIGNATURE_S),
        ])),
        nonce: Nonce(felt!(constants::NONCE)),
        resource_bounds: ValidResourceBounds::AllResources(AllResourceBounds {
            l1_gas: parse_resource_bounds(
                constants::L1_GAS_MAX_AMOUNT,
                constants::L1_GAS_MAX_PRICE,
            ),
            l2_gas: parse_resource_bounds(
                constants::L2_GAS_MAX_AMOUNT,
                constants::L2_GAS_MAX_PRICE,
            ),
            l1_data_gas: parse_resource_bounds(
                constants::L1_DATA_GAS_MAX_AMOUNT,
                constants::L1_DATA_GAS_MAX_PRICE,
            ),
        }),
        tip: Tip(parse_hex(constants::TIP)),
        calldata: Calldata(Arc::new(constants::CALLDATA.iter().map(|&s| felt!(s)).collect())),
        nonce_data_availability_mode: DataAvailabilityMode::L1,
        fee_data_availability_mode: DataAvailabilityMode::L1,
        paymaster_data: PaymasterData(vec![]),
        account_deployment_data: AccountDeploymentData(vec![]),
        proof_facts: ProofFacts::default(),
    }
}
