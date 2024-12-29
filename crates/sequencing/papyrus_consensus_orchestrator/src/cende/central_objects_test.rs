use std::sync::Arc;

use indexmap::indexmap;
use rstest::rstest;
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
use starknet_api::executable_transaction::{DeployAccountTransaction, InvokeTransaction};
use starknet_api::execution_resources::GasAmount;
use starknet_api::test_utils::read_json_file;
use starknet_api::transaction::fields::{
    AllResourceBounds,
    Calldata,
    ContractAddressSalt,
    ResourceBounds,
    ValidResourceBounds,
};
use starknet_api::transaction::{DeployAccountTransactionV3, InvokeTransactionV3, TransactionHash};
use starknet_api::{contract_address, felt, storage_key};

use super::{
    CentralBlockInfo,
    CentralDeployAccountTransaction,
    CentralInvokeTransaction,
    CentralResourcePrice,
    CentralStateDiff,
    CentralTransaction,
    CentralTransactionWritten,
};

pub const CENTRAL_STATE_DIFF_JSON_PATH: &str = "central_state_diff.json";
pub const CENTRAL_INVOKE_TX_JSON_PATH: &str = "central_invoke_tx.json";
pub const CENTRAL_DEPLOY_ACCOUNT_TX_JSON_PATH: &str = "central_deploy_account_tx.json";

fn central_state_diff() -> CentralStateDiff {
    // TODO(yael): compute the CentralStateDiff with into().
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

fn central_invoke_tx_json() -> Value {
    let invoke_tx = InvokeTransaction {
        tx: starknet_api::transaction::InvokeTransaction::V3(InvokeTransactionV3 {
            resource_bounds: ValidResourceBounds::AllResources(AllResourceBounds {
                l1_gas: ResourceBounds {
                    max_amount: GasAmount(1),
                    max_price_per_unit: GasPrice(1),
                },
                l2_gas: ResourceBounds::default(),
                l1_data_gas: ResourceBounds::default(),
            }),
            // TODO(yael): consider testing these fields with non-default values
            tip: Default::default(),
            signature: Default::default(),
            nonce: Default::default(),
            sender_address: contract_address!(
                "0x14abfd58671a1a9b30de2fcd2a42e8bff2ce1096a7c70bc7995904965f277e"
            ),
            calldata: Calldata(Arc::new(vec![felt!(0_u8), felt!(1_u8)])),
            nonce_data_availability_mode: DataAvailabilityMode::L1,
            fee_data_availability_mode: DataAvailabilityMode::L1,
            paymaster_data: Default::default(),
            account_deployment_data: Default::default(),
        }),
        tx_hash: TransactionHash(felt!(
            "0x6efd067c859e6469d0f6d158e9ae408a9552eb8cc11f618ab3aef3e52450666"
        )),
    };

    let central_transaction_written = CentralTransactionWritten {
        tx: CentralTransaction::Invoke(CentralInvokeTransaction::V3(invoke_tx.into())),
        time_created: 1734601615,
    };

    serde_json::to_value(central_transaction_written).unwrap()
}

fn central_deploy_account_tx_json() -> Value {
    let deploy_account_tx = DeployAccountTransaction {
        tx: starknet_api::transaction::DeployAccountTransaction::V3(DeployAccountTransactionV3 {
            resource_bounds: ValidResourceBounds::AllResources(AllResourceBounds {
                l1_gas: ResourceBounds {
                    max_amount: GasAmount(1),
                    max_price_per_unit: GasPrice(1),
                },
                l2_gas: ResourceBounds::default(),
                l1_data_gas: ResourceBounds::default(),
            }),
            signature: Default::default(),
            nonce: Default::default(),
            tip: Default::default(),
            paymaster_data: Default::default(),
            nonce_data_availability_mode: DataAvailabilityMode::L1,
            fee_data_availability_mode: DataAvailabilityMode::L1,

            class_hash: ClassHash(felt!(
                "0x1b5a0b09f23b091d5d1fa2f660ddfad6bcfce607deba23806cd7328ccfb8ee9"
            )),
            contract_address_salt: ContractAddressSalt(felt!(2_u8)),
            constructor_calldata: Default::default(),
        }),
        tx_hash: TransactionHash(felt!(
            "0x429cb4dc45610a80a96800ab350a11ff50e2d69e25c7723c002934e66b5a282"
        )),
        contract_address: contract_address!(
            "0x4c2e031b0ddaa38e06fd9b1bf32bff739965f9d64833006204c67cbc879a57c"
        ),
    };

    let central_transaction_written = CentralTransactionWritten {
        tx: CentralTransaction::DeployAccount(CentralDeployAccountTransaction::V3(
            deploy_account_tx.into(),
        )),
        time_created: 1734601616,
    };

    serde_json::to_value(central_transaction_written).unwrap()
}

#[rstest]
#[case::state_diff(serde_json::to_value(central_state_diff()).unwrap(), CENTRAL_STATE_DIFF_JSON_PATH)]
#[case::invoke_tx(central_invoke_tx_json(), CENTRAL_INVOKE_TX_JSON_PATH)]
#[case::deploy_account_tx(central_deploy_account_tx_json(), CENTRAL_DEPLOY_ACCOUNT_TX_JSON_PATH)]
fn serialize_central_objects(#[case] rust_json: Value, #[case] python_json_path: &str) {
    let python_json = read_json_file(python_json_path);

    assert_eq!(rust_json, python_json,);
}
