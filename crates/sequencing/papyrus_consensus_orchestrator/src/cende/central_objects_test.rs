use std::sync::Arc;

use blockifier::bouncer::{BouncerWeights, BuiltinCount};
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use indexmap::indexmap;
use rstest::rstest;
use serde_json::Value;
use starknet_api::block::{
    BlockInfo,
    BlockNumber,
    BlockTimestamp,
    GasPrice,
    GasPriceVector,
    GasPrices,
    NonzeroGasPrice,
    StarknetVersion,
};
use starknet_api::contract_class::{ClassInfo, ContractClass, SierraVersion};
use starknet_api::core::{ClassHash, CompiledClassHash, EntryPointSelector};
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::executable_transaction::{
    DeclareTransaction,
    DeployAccountTransaction,
    InvokeTransaction,
    L1HandlerTransaction,
};
use starknet_api::execution_resources::GasAmount;
use starknet_api::state::{SierraContractClass, ThinStateDiff};
use starknet_api::test_utils::read_json_file;
use starknet_api::transaction::fields::{
    AccountDeploymentData,
    AllResourceBounds,
    Calldata,
    ContractAddressSalt,
    Fee,
    PaymasterData,
    ResourceBounds,
    Tip,
    TransactionSignature,
    ValidResourceBounds,
};
use starknet_api::transaction::{
    DeclareTransactionV3,
    DeployAccountTransactionV3,
    InvokeTransactionV3,
    TransactionHash,
    TransactionVersion,
};
use starknet_api::{contract_address, felt, nonce, storage_key};
use starknet_types_core::felt::Felt;

use super::{
    CentralDeclareTransaction,
    CentralDeployAccountTransaction,
    CentralInvokeTransaction,
    CentralStateDiff,
    CentralTransaction,
    CentralTransactionWritten,
};
use crate::cende::central_objects::casm_contract_class_central_format;

pub const CENTRAL_STATE_DIFF_JSON_PATH: &str = "central_state_diff.json";
pub const CENTRAL_INVOKE_TX_JSON_PATH: &str = "central_invoke_tx.json";
pub const CENTRAL_DEPLOY_ACCOUNT_TX_JSON_PATH: &str = "central_deploy_account_tx.json";
pub const CENTRAL_DECLARE_TX_JSON_PATH: &str = "central_declare_tx.json";
pub const CENTRAL_L1_HANDLER_TX_JSON_PATH: &str = "central_l1_handler_tx.json";
pub const CENTRAL_BOUNCER_WEIGHTS_JSON_PATH: &str = "central_bouncer_weights.json";
pub const CENTRAL_SIERRA_CONTRACT_CLASS_JSON_PATH: &str = "central_sierra_contract_class.json";
pub const CENTRAL_CASM_CONTRACT_CLASS_JSON_PATH: &str = "central_contract_class.casm.json";
pub const CENTRAL_CASM_CONTRACT_CLASS_DEFAULT_OPTIONALS_JSON_PATH: &str =
    "central_contract_class_default_optionals.casm.json";

fn resource_bounds() -> ValidResourceBounds {
    ValidResourceBounds::AllResources(AllResourceBounds {
        l1_gas: ResourceBounds { max_amount: GasAmount(1), max_price_per_unit: GasPrice(1) },
        l2_gas: ResourceBounds { max_amount: GasAmount(2), max_price_per_unit: GasPrice(2) },
        l1_data_gas: ResourceBounds { max_amount: GasAmount(3), max_price_per_unit: GasPrice(3) },
    })
}

fn felt_vector() -> Vec<Felt> {
    vec![felt!(0_u8), felt!(1_u8), felt!(2_u8)]
}

fn central_state_diff_json() -> Value {
    let state_diff = ThinStateDiff {
        deployed_contracts: indexmap! {
                contract_address!(1_u8) =>
                ClassHash(felt!(1_u8)),
        },
        storage_diffs: indexmap!(contract_address!(3_u8) => indexmap!(storage_key!(3_u8) => felt!(3_u8))),
        declared_classes: indexmap!(ClassHash(felt!(4_u8))=> CompiledClassHash(felt!(4_u8))),
        nonces: indexmap!(contract_address!(2_u8)=> nonce!(2)),
        ..Default::default()
    };

    let block_info = BlockInfo {
        block_number: BlockNumber(5),
        block_timestamp: BlockTimestamp(6),
        sequencer_address: contract_address!(7_u8),
        gas_prices: GasPrices {
            eth_gas_prices: GasPriceVector {
                l1_gas_price: NonzeroGasPrice::new(GasPrice(8)).unwrap(),
                l1_data_gas_price: NonzeroGasPrice::new(GasPrice(10)).unwrap(),
                l2_gas_price: NonzeroGasPrice::new(GasPrice(12)).unwrap(),
            },
            strk_gas_prices: GasPriceVector {
                l1_gas_price: NonzeroGasPrice::new(GasPrice(9)).unwrap(),
                l1_data_gas_price: NonzeroGasPrice::new(GasPrice(11)).unwrap(),
                l2_gas_price: NonzeroGasPrice::new(GasPrice(13)).unwrap(),
            },
        },
        use_kzg_da: true,
    };

    let starknet_version = StarknetVersion::V0_13_4;

    let central_state_diff: CentralStateDiff = (state_diff, block_info, starknet_version).into();
    serde_json::to_value(central_state_diff).unwrap()
}

fn central_invoke_tx_json() -> Value {
    let invoke_tx = InvokeTransaction {
        tx: starknet_api::transaction::InvokeTransaction::V3(InvokeTransactionV3 {
            resource_bounds: resource_bounds(),
            tip: Tip(1),
            signature: TransactionSignature(felt_vector()),
            nonce: nonce!(1),
            sender_address: contract_address!(
                "0x14abfd58671a1a9b30de2fcd2a42e8bff2ce1096a7c70bc7995904965f277e"
            ),
            calldata: Calldata(Arc::new(vec![felt!(0_u8), felt!(1_u8)])),
            nonce_data_availability_mode: DataAvailabilityMode::L1,
            fee_data_availability_mode: DataAvailabilityMode::L1,
            paymaster_data: PaymasterData(vec![]),
            account_deployment_data: AccountDeploymentData(vec![]),
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
            resource_bounds: resource_bounds(),
            signature: TransactionSignature(felt_vector()),
            nonce: nonce!(1),
            tip: Tip(1),
            paymaster_data: PaymasterData(vec![]),
            nonce_data_availability_mode: DataAvailabilityMode::L1,
            fee_data_availability_mode: DataAvailabilityMode::L1,

            class_hash: ClassHash(felt!(
                "0x1b5a0b09f23b091d5d1fa2f660ddfad6bcfce607deba23806cd7328ccfb8ee9"
            )),
            contract_address_salt: ContractAddressSalt(felt!(2_u8)),
            constructor_calldata: Calldata(Arc::new(felt_vector())),
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
            resource_bounds: resource_bounds(),
            sender_address: contract_address!("0x12fd537"),
            signature: TransactionSignature(felt_vector()),
            nonce: nonce!(1),
            tip: Tip(1),
            paymaster_data: PaymasterData(vec![]),
            nonce_data_availability_mode: DataAvailabilityMode::L1,
            fee_data_availability_mode: DataAvailabilityMode::L1,
            account_deployment_data: AccountDeploymentData(vec![]),
            class_hash: ClassHash(felt!(
                "0x3a59046762823dc87385eb5ac8a21f3f5bfe4274151c6eb633737656c209056"
            )),
            compiled_class_hash: CompiledClassHash(felt!("0x1")),
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
            nonce: nonce!(1),
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

fn central_bouncer_weights_json() -> Value {
    let bouncer_weights = BouncerWeights {
        builtin_count: BuiltinCount {
            pedersen: 4948,
            poseidon: 54,
            range_check: 2301,
            ..BuiltinCount::empty()
        },
        n_events: 2,
        n_steps: 121095,
        state_diff_size: 45,
        ..BouncerWeights::empty()
    };

    serde_json::to_value(bouncer_weights).unwrap()
}

#[rstest]
#[case::state_diff(central_state_diff_json(), CENTRAL_STATE_DIFF_JSON_PATH)]
#[case::invoke_tx(central_invoke_tx_json(), CENTRAL_INVOKE_TX_JSON_PATH)]
#[case::deploy_account_tx(central_deploy_account_tx_json(), CENTRAL_DEPLOY_ACCOUNT_TX_JSON_PATH)]
#[case::declare_tx(central_declare_tx_json(), CENTRAL_DECLARE_TX_JSON_PATH)]
#[case::l1_handler_tx(central_l1_handler_tx_json(), CENTRAL_L1_HANDLER_TX_JSON_PATH)]
#[case::bouncer_weights(central_bouncer_weights_json(), CENTRAL_BOUNCER_WEIGHTS_JSON_PATH)]
fn serialize_central_objects(#[case] rust_json: Value, #[case] python_json_path: &str) {
    let python_json = read_json_file(python_json_path);

    assert_eq!(rust_json, python_json,);
}

#[test]
fn serialize_sierra_contract_class() {
    let central_sierra_contract_class = read_json_file(CENTRAL_SIERRA_CONTRACT_CLASS_JSON_PATH);
    let sierra_contract_class: SierraContractClass =
        serde_json::from_value(central_sierra_contract_class.clone()).unwrap();
    let serialized_sierra_contract_class = serde_json::to_value(&sierra_contract_class).unwrap();

    assert_eq!(central_sierra_contract_class, serialized_sierra_contract_class);
}

#[test]
fn serialize_casm_contract_class() {
    let central_casm_contract_class = read_json_file(CENTRAL_CASM_CONTRACT_CLASS_JSON_PATH);
    let casm_contract_class: CasmContractClass =
        serde_json::from_value(central_casm_contract_class.clone()).unwrap();
    let computed_central_casm_contract_class =
        casm_contract_class_central_format(casm_contract_class);
    let serialized_casm_contract_class =
        serde_json::to_value(&computed_central_casm_contract_class).unwrap();

    assert_eq!(central_casm_contract_class, serialized_casm_contract_class);
}

#[test]
fn serialize_casm_contract_class_no_optional_fields() {
    let central_casm_contract_class =
        read_json_file(CENTRAL_CASM_CONTRACT_CLASS_DEFAULT_OPTIONALS_JSON_PATH);
    let mut casm_contract_class: CasmContractClass =
        serde_json::from_value(central_casm_contract_class.clone()).unwrap();

    // Fill the optional fields with None to simulate a contract class without optional fields.
    casm_contract_class.pythonic_hints = None;
    casm_contract_class.bytecode_segment_lengths = None;

    let computed_central_casm_contract_class =
        casm_contract_class_central_format(casm_contract_class);
    let serialized_casm_contract_class =
        serde_json::to_value(&computed_central_casm_contract_class).unwrap();

    assert_eq!(central_casm_contract_class, serialized_casm_contract_class);
}
