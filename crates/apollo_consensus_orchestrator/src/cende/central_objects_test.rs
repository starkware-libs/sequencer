use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::vec;

use apollo_class_manager_types::MockClassManagerClient;
use apollo_infra_utils::test_utils::assert_json_eq;
use blockifier::execution::call_info::{
    CallExecution,
    CallInfo,
    MessageToL1,
    OrderedEvent,
    OrderedL2ToL1Message,
    Retdata,
    StorageAccessTracker,
};
use blockifier::execution::contract_class::TrackedResource;
use blockifier::execution::entry_point::{CallEntryPoint, CallType};
use blockifier::fee::fee_checks::FeeCheckError;
use blockifier::fee::receipt::TransactionReceipt;
use blockifier::fee::resources::{
    ArchivalDataResources,
    ComputationResources,
    MessageResources,
    StarknetResources,
    StateResources,
    TransactionResources,
};
use blockifier::state::cached_state::{
    CommitmentStateDiff,
    StateChangesCount,
    StateChangesCountForFee,
};
use blockifier::transaction::objects::{RevertError, TransactionExecutionInfo};
use cairo_lang_casm::hints::{CoreHint, CoreHintBase, Hint};
use cairo_lang_casm::operand::{CellRef, Register};
use cairo_lang_starknet_classes::casm_contract_class::{
    CasmContractClass,
    CasmContractEntryPoint,
    CasmContractEntryPoints,
};
use cairo_lang_starknet_classes::NestedIntList;
use cairo_lang_utils::bigint::BigUintAsHex;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use indexmap::indexmap;
use mockall::predicate::eq;
use num_bigint::BigUint;
use rstest::rstest;
use serde::Serialize;
use shared_execution_objects::central_objects::CentralTransactionExecutionInfo;
use starknet_api::block::{
    BlockHash,
    BlockInfo,
    BlockNumber,
    BlockTimestamp,
    GasPrice,
    GasPriceVector,
    GasPrices,
    NonzeroGasPrice,
    StarknetVersion,
};
use starknet_api::consensus_transaction::InternalConsensusTransaction;
use starknet_api::contract_class::{ContractClass, EntryPointType, SierraVersion};
use starknet_api::core::{ClassHash, CompiledClassHash, EntryPointSelector, EthAddress};
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::executable_transaction::L1HandlerTransaction;
use starknet_api::execution_resources::{GasAmount, GasVector};
use starknet_api::rpc_transaction::{
    EntryPointByType,
    InternalRpcDeclareTransactionV3,
    InternalRpcDeployAccountTransaction,
    InternalRpcTransaction,
    InternalRpcTransactionWithoutTxHash,
    RpcDeployAccountTransaction,
    RpcDeployAccountTransactionV3,
    RpcInvokeTransaction,
    RpcInvokeTransactionV3,
};
use starknet_api::state::{
    EntryPoint,
    FunctionIndex,
    SierraContractClass,
    StorageKey,
    ThinStateDiff,
};
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
};
use starknet_api::transaction::{
    EventContent,
    EventData,
    EventKey,
    L2ToL1Payload,
    TransactionHash,
    TransactionVersion,
};
use starknet_api::{contract_address, felt, nonce, storage_key};
use starknet_types_core::felt::Felt;

use super::{
    CentralBouncerWeights,
    CentralCasmHashComputationData,
    CentralCompressedStateDiff,
    CentralDeclareTransaction,
    CentralDeployAccountTransaction,
    CentralFeeMarketInfo,
    CentralInvokeTransaction,
    CentralSierraContractClass,
    CentralStateDiff,
    CentralTransaction,
    CentralTransactionWritten,
};
use crate::cende::central_objects::CentralCasmContractClass;
use crate::cende::{AerospikeBlob, BlobParameters};

// TODO(yael, dvir): add default object serialization tests.

pub const CENTRAL_STATE_DIFF_JSON_PATH: &str = "central_state_diff.json";
pub const CENTRAL_INVOKE_TX_JSON_PATH: &str = "central_invoke_tx.json";
pub const CENTRAL_DEPLOY_ACCOUNT_TX_JSON_PATH: &str = "central_deploy_account_tx.json";
pub const CENTRAL_DECLARE_TX_JSON_PATH: &str = "central_declare_tx.json";
pub const CENTRAL_L1_HANDLER_TX_JSON_PATH: &str = "central_l1_handler_tx.json";
pub const CENTRAL_BOUNCER_WEIGHTS_JSON_PATH: &str = "central_bouncer_weights.json";
pub const CENTRAL_FEE_MARKET_INFO_JSON_PATH: &str = "central_fee_market_info.json";
pub const CENTRAL_SIERRA_CONTRACT_CLASS_JSON_PATH: &str = "central_contract_class.sierra.json";
pub const CENTRAL_CASM_CONTRACT_CLASS_JSON_PATH: &str = "central_contract_class.casm.json";
pub const CENTRAL_CASM_CONTRACT_CLASS_DEFAULT_OPTIONALS_JSON_PATH: &str =
    "central_contract_class_default_optionals.casm.json";
pub const CENTRAL_TRANSACTION_EXECUTION_INFO_JSON_PATH: &str =
    "central_transaction_execution_info.json";
pub const CENTRAL_TRANSACTION_EXECUTION_INFO_REVERTED_JSON_PATH: &str =
    "central_transaction_execution_info_reverted.json";
pub const CENTRAL_BLOB_JSON_PATH: &str = "central_blob.json";
pub const CENTRAL_CASM_HASH_COMPUTATION_DATA_JSON_PATH: &str =
    "central_casm_hash_computation_data.json";

fn resource_bounds() -> AllResourceBounds {
    AllResourceBounds {
        l1_gas: ResourceBounds { max_amount: GasAmount(1), max_price_per_unit: GasPrice(1) },
        l2_gas: ResourceBounds { max_amount: GasAmount(2), max_price_per_unit: GasPrice(2) },
        l1_data_gas: ResourceBounds { max_amount: GasAmount(3), max_price_per_unit: GasPrice(3) },
    }
}

fn felt_vector() -> Vec<Felt> {
    vec![felt!(0_u8), felt!(1_u8), felt!(2_u8)]
}

fn declare_class_hash() -> ClassHash {
    ClassHash(felt!("0x3a59046762823dc87385eb5ac8a21f3f5bfe4274151c6eb633737656c209056"))
}

fn declare_compiled_class_hash() -> CompiledClassHash {
    CompiledClassHash(felt!(1_u8))
}

fn thin_state_diff() -> ThinStateDiff {
    ThinStateDiff {
        deployed_contracts: indexmap! {
                contract_address!(1_u8) =>
                ClassHash(felt!(1_u8)),
                contract_address!(5_u8)=> ClassHash(felt!(5_u8)),
        },
        storage_diffs: indexmap!(contract_address!(3_u8) => indexmap!(storage_key!(3_u8) => felt!(3_u8))),
        declared_classes: indexmap!(ClassHash(felt!(4_u8))=> CompiledClassHash(felt!(4_u8))),
        nonces: indexmap!(contract_address!(2_u8)=> nonce!(2)),
        ..Default::default()
    }
}

fn block_info() -> BlockInfo {
    BlockInfo {
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
    }
}

fn central_state_diff() -> CentralStateDiff {
    let state_diff = thin_state_diff();
    let block_info = block_info();
    let starknet_version = StarknetVersion::V0_14_1;

    (state_diff, (block_info, starknet_version).into()).into()
}

fn commitment_state_diff() -> CommitmentStateDiff {
    CommitmentStateDiff {
        address_to_class_hash: indexmap! {
                contract_address!(1_u8) => ClassHash(felt!(1_u8)),
                contract_address!(5_u8)=> ClassHash(felt!(5_u8)),
        },
        storage_updates: indexmap!(contract_address!(3_u8) => indexmap!(storage_key!(3_u8) => felt!(3_u8))),
        class_hash_to_compiled_class_hash: indexmap!(ClassHash(felt!(4_u8))=> CompiledClassHash(felt!(4_u8))),
        address_to_nonce: indexmap!(contract_address!(2_u8)=> nonce!(2)),
    }
}

fn central_compressed_state_diff() -> CentralCompressedStateDiff {
    let state_diff = commitment_state_diff();
    let block_info = block_info();
    let starknet_version = StarknetVersion::V0_14_1;

    (state_diff, (block_info, starknet_version).into()).into()
}

fn invoke_transaction() -> RpcInvokeTransaction {
    RpcInvokeTransaction::V3(RpcInvokeTransactionV3 {
        resource_bounds: resource_bounds(),
        tip: Tip(1),
        signature: TransactionSignature(felt_vector().into()),
        nonce: nonce!(1),
        sender_address: contract_address!(
            "0x14abfd58671a1a9b30de2fcd2a42e8bff2ce1096a7c70bc7995904965f277e"
        ),
        calldata: Calldata(Arc::new(vec![felt!(0_u8), felt!(1_u8)])),
        nonce_data_availability_mode: DataAvailabilityMode::L1,
        fee_data_availability_mode: DataAvailabilityMode::L1,
        paymaster_data: PaymasterData(vec![]),
        account_deployment_data: AccountDeploymentData(vec![]),
    })
}

fn central_invoke_tx() -> CentralTransactionWritten {
    let invoke_tx = invoke_transaction();
    let tx_hash =
        TransactionHash(felt!("0x6efd067c859e6469d0f6d158e9ae408a9552eb8cc11f618ab3aef3e52450666"));

    CentralTransactionWritten {
        tx: CentralTransaction::Invoke(CentralInvokeTransaction::V3((invoke_tx, tx_hash).into())),
        time_created: 1734601615,
    }
}

fn deploy_account_tx() -> InternalRpcDeployAccountTransaction {
    InternalRpcDeployAccountTransaction {
        tx: RpcDeployAccountTransaction::V3(RpcDeployAccountTransactionV3 {
            resource_bounds: resource_bounds(),
            tip: Tip(1),
            signature: TransactionSignature(felt_vector().into()),
            nonce: nonce!(1),
            class_hash: ClassHash(felt!(
                "0x1b5a0b09f23b091d5d1fa2f660ddfad6bcfce607deba23806cd7328ccfb8ee9"
            )),
            contract_address_salt: ContractAddressSalt(felt!(2_u8)),
            constructor_calldata: Calldata(Arc::new(felt_vector())),
            nonce_data_availability_mode: DataAvailabilityMode::L1,
            fee_data_availability_mode: DataAvailabilityMode::L1,
            paymaster_data: PaymasterData(vec![]),
        }),
        contract_address: contract_address!(
            "0x4c2e031b0ddaa38e06fd9b1bf32bff739965f9d64833006204c67cbc879a57c"
        ),
    }
}

fn central_deploy_account_tx() -> CentralTransactionWritten {
    let deploy_account_tx = deploy_account_tx();

    let tx_hash =
        TransactionHash(felt!("0x429cb4dc45610a80a96800ab350a11ff50e2d69e25c7723c002934e66b5a282"));

    CentralTransactionWritten {
        tx: CentralTransaction::DeployAccount(CentralDeployAccountTransaction::V3(
            (deploy_account_tx, tx_hash).into(),
        )),
        time_created: 1734601616,
    }
}

fn declare_transaction() -> InternalRpcDeclareTransactionV3 {
    InternalRpcDeclareTransactionV3 {
        resource_bounds: resource_bounds(),
        tip: Tip(1),
        signature: TransactionSignature(felt_vector().into()),
        nonce: nonce!(1),
        class_hash: declare_class_hash(),
        compiled_class_hash: declare_compiled_class_hash(),
        sender_address: contract_address!("0x12fd537"),
        nonce_data_availability_mode: DataAvailabilityMode::L1,
        fee_data_availability_mode: DataAvailabilityMode::L1,
        paymaster_data: PaymasterData(vec![]),
        account_deployment_data: AccountDeploymentData(vec![]),
    }
}

fn central_declare_tx() -> CentralTransactionWritten {
    let tx_hash =
        TransactionHash(felt!("0x41e7d973115400a98a7775190c27d4e3b1fcd8cd40b7d27464f6c3f10b8b706"));
    let declare_tx = declare_transaction();

    CentralTransactionWritten {
        tx: CentralTransaction::Declare(CentralDeclareTransaction::V3(
            (declare_tx, &sierra_contract_class(), tx_hash).try_into().unwrap(),
        )),
        time_created: 1734601649,
    }
}

fn l1_handler_tx() -> L1HandlerTransaction {
    L1HandlerTransaction {
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
    }
}

fn central_l1_handler_tx() -> CentralTransactionWritten {
    let l1_handler_tx = l1_handler_tx();

    CentralTransactionWritten {
        tx: CentralTransaction::L1Handler(l1_handler_tx.into()),
        time_created: 1734601657,
    }
}

fn central_bouncer_weights() -> CentralBouncerWeights {
    CentralBouncerWeights {
        l1_gas: 8,
        message_segment_length: 9,
        n_events: 2,
        state_diff_size: 45,
        sierra_gas: GasAmount(10),
        n_txs: 2,
    }
}

fn central_fee_market_info() -> CentralFeeMarketInfo {
    CentralFeeMarketInfo { l2_gas_consumed: GasAmount(150000), next_l2_gas_price: GasPrice(100000) }
}

fn entry_point(idx: usize, selector: u8) -> EntryPoint {
    EntryPoint { function_idx: FunctionIndex(idx), selector: EntryPointSelector(felt!(selector)) }
}

fn sierra_contract_class() -> SierraContractClass {
    SierraContractClass {
        sierra_program: felt_vector(),
        contract_class_version: "0.1.0".to_string(),
        entry_points_by_type: EntryPointByType {
            constructor: vec![entry_point(1, 2)],
            external: vec![entry_point(3, 4)],
            l1handler: vec![entry_point(5, 6)],
        },
        abi: "dummy abi".to_string(),
    }
}

fn central_casm_hash_computation_data() -> CentralCasmHashComputationData {
    CentralCasmHashComputationData {
        class_hash_to_casm_hash_computation_gas: HashMap::from([(
            declare_class_hash(),
            GasAmount(1),
        )]),
        sierra_gas_without_casm_hash_computation: GasAmount(3),
        // TODO(Meshi): Change to relevant test case when the migration is implemented.
        class_hashes_for_migration: HashSet::default(),
    }
}

fn central_sierra_contract_class() -> CentralSierraContractClass {
    CentralSierraContractClass { contract_class: sierra_contract_class() }
}

fn casm_contract_entry_points() -> Vec<CasmContractEntryPoint> {
    vec![CasmContractEntryPoint {
        selector: BigUint::from(1_u8),
        offset: 1,
        builtins: vec!["dummy builtin".to_string()],
    }]
}

fn casm_contract_class() -> CasmContractClass {
    CasmContractClass {
        prime: BigUint::from(1_u8),
        compiler_version: "dummy version".to_string(),
        bytecode: vec![BigUintAsHex { value: BigUint::from(1_u8) }],
        bytecode_segment_lengths: Some(NestedIntList::Node(vec![
            NestedIntList::Leaf(1),
            NestedIntList::Leaf(2),
        ])),
        hints: vec![(
            4,
            vec![Hint::Core(CoreHintBase::Core(CoreHint::AllocSegment {
                dst: CellRef { register: Register::AP, offset: 1 },
            }))],
        )],
        pythonic_hints: Some(vec![(5, vec!["dummy pythonic hint".to_string()])]),
        entry_points_by_type: CasmContractEntryPoints {
            external: casm_contract_entry_points(),
            l1_handler: casm_contract_entry_points(),
            constructor: casm_contract_entry_points(),
        },
    }
}

fn central_casm_contract_class() -> CentralCasmContractClass {
    CentralCasmContractClass::from(casm_contract_class())
}

fn central_casm_contract_class_default_optional_fields() -> CentralCasmContractClass {
    let casm_contract_class = CasmContractClass {
        bytecode_segment_lengths: None,
        pythonic_hints: None,
        ..casm_contract_class()
    };
    CentralCasmContractClass::from(casm_contract_class)
}

fn execution_resources() -> ExecutionResources {
    ExecutionResources {
        n_steps: 2,
        n_memory_holes: 3,
        builtin_instance_counter: HashMap::from([
            (BuiltinName::range_check, 31),
            (BuiltinName::pedersen, 4),
        ]),
    }
}

fn call_info() -> CallInfo {
    CallInfo {
        call: CallEntryPoint {
            class_hash: Some(ClassHash(felt!("0x80020000"))),
            code_address: Some(contract_address!("0x40070000")),
            entry_point_type: EntryPointType::External,
            entry_point_selector: EntryPointSelector(felt!(
                "0x162da33a4585851fe8d3af3c2a9c60b557814e221e0d4f30ff0b2189d9c7775"
            )),
            calldata: Calldata(Arc::new(vec![
                felt!("0x40070000"),
                felt!("0x39a1491f76903a16feed0a6433bec78de4c73194944e1118e226820ad479701"),
                felt!("0x1"),
                felt!("0x2"),
            ])),
            storage_address: contract_address!("0xc0020000"),
            caller_address: contract_address!("0x1"),
            call_type: CallType::Call,
            initial_gas: 100_000_000,
        },
        execution: CallExecution {
            retdata: Retdata(vec![felt!("0x56414c4944")]),
            events: vec![OrderedEvent {
                order: 2,
                event: EventContent {
                    keys: vec![EventKey(felt!("0x9"))],
                    data: EventData(felt_vector()),
                },
            }],
            l2_to_l1_messages: vec![OrderedL2ToL1Message {
                order: 1,
                message: MessageToL1 {
                    to_address: EthAddress::try_from(felt!(1_u8)).unwrap(),
                    payload: L2ToL1Payload(felt_vector()),
                },
            }],
            failed: false,
            gas_consumed: 11_690,
        },
        inner_calls: Vec::new(),
        resources: execution_resources(),
        tracked_resource: TrackedResource::SierraGas,
        storage_access_tracker: StorageAccessTracker {
            storage_read_values: felt_vector(),
            accessed_storage_keys: HashSet::from([StorageKey::from(1_u128)]),
            read_class_hash_values: vec![ClassHash(felt!("0x80020000"))],
            accessed_contract_addresses: HashSet::from([contract_address!("0x1")]),
            read_block_hash_values: vec![BlockHash(felt!("0xdeafbee"))],
            accessed_blocks: HashSet::from([BlockNumber(100)]),
        },
    }
}

// This object is very long , so in order to test all types of sub-structs and refrain from filling
// the entire object, we fill only one CallInfo with non-default values and the other CallInfos are
// None.
fn transaction_execution_info() -> TransactionExecutionInfo {
    TransactionExecutionInfo {
        validate_call_info: Some(CallInfo { inner_calls: vec![call_info()], ..call_info() }),
        execute_call_info: Some(CallInfo { inner_calls: vec![call_info()], ..call_info() }),
        fee_transfer_call_info: Some(CallInfo { inner_calls: vec![call_info()], ..call_info() }),
        revert_error: None,
        receipt: TransactionReceipt {
            fee: Fee(0x26fe9d250e000),
            gas: GasVector {
                l1_gas: GasAmount(6860),
                l1_data_gas: GasAmount(1),
                l2_gas: GasAmount(1),
            },
            da_gas: GasVector {
                l1_gas: GasAmount(1652),
                l1_data_gas: GasAmount(1),
                l2_gas: GasAmount(1),
            },
            resources: TransactionResources {
                starknet_resources: StarknetResources {
                    // The archival_data has private fields so it cannot be assigned, however, it is
                    // not being used in the central object anyway so it can be default.
                    archival_data: ArchivalDataResources::default(),
                    messages: MessageResources {
                        l2_to_l1_payload_lengths: vec![1, 2],
                        message_segment_length: 1,
                        l1_handler_payload_size: Some(1),
                    },
                    state: StateResources {
                        state_changes_for_fee: StateChangesCountForFee {
                            state_changes_count: StateChangesCount {
                                n_storage_updates: 1,
                                n_class_hash_updates: 2,
                                n_compiled_class_hash_updates: 3,
                                n_modified_contracts: 4,
                            },
                            n_allocated_keys: 5,
                        },
                    },
                },
                computation: ComputationResources {
                    vm_resources: execution_resources(),
                    n_reverted_steps: 2,
                    sierra_gas: GasAmount(0x128140),
                    reverted_sierra_gas: GasAmount(0x2),
                },
            },
        },
    }
}

fn central_transaction_execution_info() -> CentralTransactionExecutionInfo {
    transaction_execution_info().into()
}

fn central_transaction_execution_info_reverted() -> CentralTransactionExecutionInfo {
    let mut transaction_execution_info = transaction_execution_info();
    // The python side enforces that if the transaction is reverted, the execute_call_info is None.
    // Since we are using the same json files for python tests, we apply these rules here as well.
    transaction_execution_info.execute_call_info = None;

    transaction_execution_info.revert_error =
        Some(RevertError::PostExecution(FeeCheckError::InsufficientFeeTokenBalance {
            fee: Fee(1),
            balance_low: felt!(2_u8),
            balance_high: felt!(3_u8),
        }));

    transaction_execution_info.into()
}

fn declare_tx_with_hash(tx_hash: u64) -> InternalConsensusTransaction {
    InternalConsensusTransaction::RpcTransaction(InternalRpcTransaction {
        tx: InternalRpcTransactionWithoutTxHash::Declare(declare_transaction()),
        tx_hash: TransactionHash(felt!(tx_hash)),
    })
}

// Returns a vector of transactions and a mock class manager with the expectation that needed to
// convert the consensus transactions to central transactions.
fn input_txs_and_mock_class_manager() -> (Vec<InternalConsensusTransaction>, MockClassManagerClient)
{
    let invoke = InternalConsensusTransaction::RpcTransaction(InternalRpcTransaction {
        tx: InternalRpcTransactionWithoutTxHash::Invoke(invoke_transaction()),
        tx_hash: TransactionHash(Felt::TWO),
    });
    let deploy_account = InternalConsensusTransaction::RpcTransaction(InternalRpcTransaction {
        tx: InternalRpcTransactionWithoutTxHash::DeployAccount(deploy_account_tx()),
        tx_hash: TransactionHash(Felt::THREE),
    });
    let l1_handler = InternalConsensusTransaction::L1Handler(l1_handler_tx());

    let transactions =
        vec![declare_tx_with_hash(1), invoke, deploy_account, l1_handler, declare_tx_with_hash(4)];

    let mut mock_class_manager = MockClassManagerClient::new();
    mock_class_manager
        .expect_get_sierra()
        .with(eq(declare_class_hash()))
        .times(2)
        .returning(|_| Ok(Some(sierra_contract_class())));
    mock_class_manager.expect_get_executable().with(eq(declare_class_hash())).times(2).returning(
        |_| Ok(Some(ContractClass::V1((casm_contract_class(), SierraVersion::new(0, 0, 0))))),
    );

    (transactions, mock_class_manager)
}

// TODO(dvir): use real blob when possible.
fn central_blob() -> AerospikeBlob {
    let (input_txs, mock_class_manager) = input_txs_and_mock_class_manager();
    let blob_parameters = BlobParameters {
        block_info: block_info(),
        state_diff: thin_state_diff(),
        compressed_state_diff: Some(commitment_state_diff()),
        transactions: input_txs,
        bouncer_weights: central_bouncer_weights(),
        fee_market_info: central_fee_market_info(),
        execution_infos: vec![transaction_execution_info()],
        casm_hash_computation_data: central_casm_hash_computation_data(),
    };

    // This is to make the function sync (not async) so that it can be used as a case in the
    // serialize_central_objects test.
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime
        .block_on(AerospikeBlob::from_blob_parameters_and_class_manager(
            blob_parameters,
            Arc::new(mock_class_manager),
        ))
        .unwrap()
}

#[rstest]
#[case::compressed_state_diff(central_compressed_state_diff(), CENTRAL_STATE_DIFF_JSON_PATH)]
#[case::state_diff(central_state_diff(), CENTRAL_STATE_DIFF_JSON_PATH)]
#[case::invoke_tx(central_invoke_tx(), CENTRAL_INVOKE_TX_JSON_PATH)]
#[case::deploy_account_tx(central_deploy_account_tx(), CENTRAL_DEPLOY_ACCOUNT_TX_JSON_PATH)]
#[case::declare_tx(central_declare_tx(), CENTRAL_DECLARE_TX_JSON_PATH)]
#[case::l1_handler_tx(central_l1_handler_tx(), CENTRAL_L1_HANDLER_TX_JSON_PATH)]
#[case::bouncer_weights(central_bouncer_weights(), CENTRAL_BOUNCER_WEIGHTS_JSON_PATH)]
#[case::fee_market_info(central_fee_market_info(), CENTRAL_FEE_MARKET_INFO_JSON_PATH)]
#[case::sierra_contract_class(
    central_sierra_contract_class(),
    CENTRAL_SIERRA_CONTRACT_CLASS_JSON_PATH
)]
#[case::optionals_are_some(central_casm_contract_class(), CENTRAL_CASM_CONTRACT_CLASS_JSON_PATH)]
#[case::optionals_are_none(
    central_casm_contract_class_default_optional_fields(),
    CENTRAL_CASM_CONTRACT_CLASS_DEFAULT_OPTIONALS_JSON_PATH
)]
#[case::transaction_execution_info(
    central_transaction_execution_info(),
    CENTRAL_TRANSACTION_EXECUTION_INFO_JSON_PATH
)]
#[case::transaction_execution_info_reverted(
    central_transaction_execution_info_reverted(),
    CENTRAL_TRANSACTION_EXECUTION_INFO_REVERTED_JSON_PATH
)]
#[case::casm_hash_computation_data(
    central_casm_hash_computation_data(),
    CENTRAL_CASM_HASH_COMPUTATION_DATA_JSON_PATH
)]
#[case::central_blob(central_blob(), CENTRAL_BLOB_JSON_PATH)]
fn serialize_central_objects(#[case] rust_obj: impl Serialize, #[case] python_json_path: &str) {
    let python_json = read_json_file(python_json_path);
    let rust_json = serde_json::to_value(rust_obj).unwrap();

    assert_json_eq(&rust_json, &python_json, "Json Comparison failed".to_string());
}
