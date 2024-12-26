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
use starknet_api::contract_class::{ClassInfo, ContractClass, SierraVersion};
use starknet_api::core::{ClassHash, CompiledClassHash, EntryPointSelector, Nonce};
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::executable_transaction::{
    DeclareTransaction,
    DeployAccountTransaction,
    InvokeTransaction,
    L1HandlerTransaction,
};
use starknet_api::execution_resources::GasAmount;
use starknet_api::test_utils::read_json_file;
use starknet_api::transaction::fields::{
    AllResourceBounds,
    Calldata,
    ContractAddressSalt,
    Fee,
    ResourceBounds,
    ValidResourceBounds,
};
use starknet_api::transaction::{
    DeclareTransactionV3,
    DeployAccountTransactionV3,
    InvokeTransactionV3,
    TransactionHash,
    TransactionVersion,
};
use starknet_api::{contract_address, felt, storage_key};

use super::{
    CentralBlockInfo,
    CentralDeclareTransaction,
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
pub const CENTRAL_DECLARE_TX_JSON_PATH: &str = "central_declare_tx.json";
pub const CENTRAL_L1_HANDLER_TX_JSON_PATH: &str = "central_l1_handler_tx.json";

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

fn central_declare_tx_json() -> Value {
    let declare_tx = DeclareTransaction {
        tx: starknet_api::transaction::DeclareTransaction::V3(DeclareTransactionV3 {
            resource_bounds: ValidResourceBounds::AllResources(AllResourceBounds {
                l1_gas: ResourceBounds {
                    max_amount: GasAmount(1),
                    max_price_per_unit: GasPrice(1),
                },
                l2_gas: ResourceBounds::default(),
                l1_data_gas: ResourceBounds::default(),
            }),
            sender_address: contract_address!("0x12fd537"),
            signature: Default::default(),
            nonce: Nonce(felt!("0x0")),
            tip: Default::default(),
            paymaster_data: Default::default(),
            nonce_data_availability_mode: DataAvailabilityMode::L1,
            fee_data_availability_mode: DataAvailabilityMode::L1,
            account_deployment_data: Default::default(),
            class_hash: ClassHash(felt!(
                "0x3a59046762823dc87385eb5ac8a21f3f5bfe4274151c6eb633737656c209056"
            )),
            compiled_class_hash: CompiledClassHash(felt!("0x0")),
        }),
        tx_hash: TransactionHash(felt!(
            "0x41e7d973115400a98a7775190c27d4e3b1fcd8cd40b7d27464f6c3f10b8b706"
        )),
        class_info: ClassInfo {
            // The contract class is not used by the central object.
            contract_class: ContractClass::V0(Default::default()),
            sierra_program_length: 8844,
            abi_length: 11237,
            sierra_version: SierraVersion::new(1, 6, 0),
        },
    };
    let central_transaction_written = CentralTransactionWritten {
        tx: CentralTransaction::Declare(CentralDeclareTransaction::V3(declare_tx.into())),
        time_created: 1734601649,
    };

    serde_json::to_value(central_transaction_written).unwrap()
}

fn central_l1_handler_tx_json() -> Value {
    let l1_handler_tx = L1HandlerTransaction {
        tx: starknet_api::transaction::L1HandlerTransaction {
            version: TransactionVersion::ZERO,
            nonce: Default::default(),
            contract_address: contract_address!(
                "0x14abfd58671a1a9b30de2fcd2a42e8bff2ce1096a7c70bc7995904965f277e"
            ),
            entry_point_selector: EntryPointSelector(felt!("0x2a")),
            calldata: Calldata(Arc::new(vec![felt!(0_u8), felt!(1_u8)])),
        },
        tx_hash: TransactionHash(felt!(
            "0xc947753befd252ca08042000cd6d783162ee2f5df87b519ddf3081b9b4b997"
        )),
        paid_fee_on_l1: Fee(1),
    };
    let central_transaction_written = CentralTransactionWritten {
        tx: CentralTransaction::L1Handler(l1_handler_tx.into()),
        time_created: 1734601657,
    };

    serde_json::to_value(central_transaction_written).unwrap()
}

#[rstest]
#[case::state_diff(serde_json::to_value(central_state_diff()).unwrap(), CENTRAL_STATE_DIFF_JSON_PATH)]
#[case::invoke_tx(central_invoke_tx_json(), CENTRAL_INVOKE_TX_JSON_PATH)]
#[case::deploy_account_tx(central_deploy_account_tx_json(), CENTRAL_DEPLOY_ACCOUNT_TX_JSON_PATH)]
#[case::declare_tx(central_declare_tx_json(), CENTRAL_DECLARE_TX_JSON_PATH)]
#[case::l1_handler_tx(central_l1_handler_tx_json(), CENTRAL_L1_HANDLER_TX_JSON_PATH)]
fn serialize_central_objects(#[case] rust_json: Value, #[case] python_json_path: &str) {
    let python_json = read_json_file(python_json_path);

    assert_eq!(rust_json, python_json,);
}
