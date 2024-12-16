use std::fs;

use indexmap::indexmap;
use infra_utils::path::resolve_project_relative_path;
use serde_json::Value;
use starknet_api::block::{
    BlockNumber,
    BlockTimestamp,
    GasPrice,
    NonzeroGasPrice,
    StarknetVersion,
};
use starknet_api::core::{ClassHash, CompiledClassHash, Nonce};
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::{contract_address, felt, storage_key};

use super::central_objects::{CentralBlockInfo, CentralResourcePrice, CentralStateDiff};

pub const CENTRAL_STATE_DIFF_JSON_PATH: &str = "crates/sequencing/papyrus_consensus_orchestrator/\
                                                src/central_communication/resources/\
                                                central_state_diff.json";

fn central_state_diff() -> CentralStateDiff {
    CentralStateDiff {
        address_to_class_hash: indexmap! {
                contract_address!(1_u8) =>
                ClassHash(felt!(1_u8)),
        },
        nonces: indexmap!(
            DataAvailabilityMode::L1 =>
            indexmap!(contract_address!(2_u8)=> Nonce(felt!(2_u8))),
        ),
        storage_updates: indexmap!(
            DataAvailabilityMode::L1=>
            indexmap!(contract_address!(3_u8) => indexmap!(storage_key!(3_u8) => felt!(3_u8))),
        ),
        declared_classes: indexmap!(ClassHash(felt!(4_u8))=> CompiledClassHash(felt!(4_u8))),
        block_info: CentralBlockInfo {
            block_number: BlockNumber(5),
            block_timestamp: BlockTimestamp(6),
            sequencer_address: contract_address!(7_u8),
            l1_gas_price: CentralResourcePrice {
                price_in_wei: NonzeroGasPrice::new(GasPrice(8)).unwrap(),
                price_in_fri: NonzeroGasPrice::new(GasPrice(9)).unwrap(),
            },
            l1_data_gas_price: CentralResourcePrice {
                price_in_wei: NonzeroGasPrice::new(GasPrice(10)).unwrap(),
                price_in_fri: NonzeroGasPrice::new(GasPrice(11)).unwrap(),
            },
            l2_gas_price: CentralResourcePrice {
                price_in_wei: NonzeroGasPrice::new(GasPrice(12)).unwrap(),
                price_in_fri: NonzeroGasPrice::new(GasPrice(13)).unwrap(),
            },
            use_kzg_da: true,
            starknet_version: Some(StarknetVersion::default()),
        },
    }
}

#[test]
fn serialize_central_state_diff() {
    let rust_central_state_diff = central_state_diff();

    let rust_serialized = serde_json::to_string(&rust_central_state_diff).unwrap();
    let rust_json: Value = serde_json::from_str(&rust_serialized).unwrap();

    let python_json_string =
        fs::read_to_string(resolve_project_relative_path(CENTRAL_STATE_DIFF_JSON_PATH).unwrap())
            .unwrap();
    let python_json: Value = serde_json::from_str(&python_json_string).unwrap();

    assert_eq!(rust_json, python_json,);
}
