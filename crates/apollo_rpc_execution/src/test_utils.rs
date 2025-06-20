use std::collections::HashMap;

use apollo_storage::body::BodyStorageWriter;
use apollo_storage::class::ClassStorageWriter;
use apollo_storage::compiled_class::CasmStorageWriter;
use apollo_storage::header::HeaderStorageWriter;
use apollo_storage::state::StateStorageWriter;
use apollo_storage::{StorageReader, StorageWriter};
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use indexmap::indexmap;
use lazy_static::lazy_static;
use serde::de::DeserializeOwned;
use starknet_api::abi::abi_utils::get_storage_var_address;
use starknet_api::block::{
    BlockBody,
    BlockHash,
    BlockHeader,
    BlockHeaderWithoutHash,
    BlockNumber,
    BlockTimestamp,
    GasPrice,
    GasPricePerToken,
};
use starknet_api::contract_class::SierraVersion;
use starknet_api::core::{
    ChainId,
    ClassHash,
    CompiledClassHash,
    ContractAddress,
    Nonce,
    SequencerContractAddress,
};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::state::{SierraContractClass, StateNumber, ThinStateDiff};
use starknet_api::test_utils::read_json_file;
use starknet_api::transaction::fields::Fee;
use starknet_api::transaction::{
    DeclareTransactionV0V1,
    DeclareTransactionV2,
    DeployAccountTransaction,
    DeployAccountTransactionV1,
    InvokeTransaction,
    InvokeTransactionV1,
    TransactionHash,
};
use starknet_api::{calldata, class_hash, contract_address, felt, nonce};
use starknet_types_core::felt::Felt;

use crate::execution_utils::selector_from_name;
use crate::objects::{PendingData, TransactionSimulationOutput};
use crate::testing_instances::get_test_execution_config;
use crate::{simulate_transactions, ExecutableTransactionInput, OnlyQuery, SierraSize};

lazy_static! {
    pub static ref CHAIN_ID: ChainId = ChainId::Other(String::from("TEST_CHAIN_ID"));
    pub static ref GAS_PRICE: GasPricePerToken = GasPricePerToken{
        price_in_wei: (100 * u128::pow(10, 9)).into(),
        // TODO(yair): add value and tests.
        price_in_fri: GasPrice::default(),
    };
    pub static ref MAX_FEE: Fee = Fee(1000000 * GAS_PRICE.price_in_wei.0);
    pub static ref BLOCK_TIMESTAMP: BlockTimestamp = BlockTimestamp(1234);
    pub static ref SEQUENCER_ADDRESS: SequencerContractAddress =
        SequencerContractAddress(contract_address!("0xa"));
    pub static ref DEPRECATED_CONTRACT_ADDRESS: ContractAddress = contract_address!("0x1");
    pub static ref CONTRACT_ADDRESS: ContractAddress = contract_address!("0x2");
    pub static ref ACCOUNT_CLASS_HASH: ClassHash = class_hash!("0x333");
    pub static ref ACCOUNT_ADDRESS: ContractAddress = contract_address!("0x444");
    // Taken from the trace of the deploy account transaction.
    pub static ref NEW_ACCOUNT_ADDRESS: ContractAddress =
        contract_address!("0x0153ade9ef510502c4f3b879c049dcc3ad5866706cae665f0d9df9b01e794fdb");
    pub static ref TEST_ERC20_CONTRACT_CLASS_HASH: ClassHash = class_hash!("0x1010");
    pub static ref TEST_ERC20_CONTRACT_ADDRESS: ContractAddress = contract_address!("0x1001");
    pub static ref ACCOUNT_INITIAL_BALANCE: Felt = felt!(2 * MAX_FEE.0);
}

// Sierra size must be > 0.
const DUMMY_SIERRA_SIZE: SierraSize = 1;

fn get_test_instance<T: DeserializeOwned>(path_in_resource_dir: &str) -> T {
    read_json_file(path_in_resource_dir)
}

// A deprecated class for testing, taken from get_deprecated_contract_class of Blockifier.
pub fn get_test_deprecated_contract_class() -> DeprecatedContractClass {
    get_test_instance("deprecated_class.json")
}
pub fn get_test_casm() -> CasmContractClass {
    get_test_instance("casm.json")
}
pub fn get_test_erc20_fee_contract_class() -> DeprecatedContractClass {
    get_test_instance("erc20_fee_contract_class.json")
}
// An account class for testing.
pub fn get_test_account_class() -> DeprecatedContractClass {
    get_test_instance("account_class.json")
}

pub fn prepare_storage(mut storage_writer: StorageWriter) {
    let class_hash0 = class_hash!("0x2");
    let class_hash1 = class_hash!("0x1");

    let minter_var_address = get_storage_var_address("permitted_minter", &[]);

    let account_balance_key =
        get_storage_var_address("ERC20_balances", &[*ACCOUNT_ADDRESS.0.key()]);
    let new_account_balance_key =
        get_storage_var_address("ERC20_balances", &[*NEW_ACCOUNT_ADDRESS.0.key()]);

    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(
            BlockNumber(0),
            &BlockHeader {
                block_header_without_hash: BlockHeaderWithoutHash {
                    l1_gas_price: *GAS_PRICE,
                    sequencer: *SEQUENCER_ADDRESS,
                    timestamp: *BLOCK_TIMESTAMP,
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .unwrap()
        .append_body(BlockNumber(0), BlockBody::default())
        .unwrap()
        .append_state_diff(
            BlockNumber(0),
            ThinStateDiff {
                deployed_contracts: indexmap!(
                    *TEST_ERC20_CONTRACT_ADDRESS => *TEST_ERC20_CONTRACT_CLASS_HASH,
                    *CONTRACT_ADDRESS => class_hash0,
                    *DEPRECATED_CONTRACT_ADDRESS => class_hash1,
                    *ACCOUNT_ADDRESS => *ACCOUNT_CLASS_HASH,
                ),
                storage_diffs: indexmap!(
                    *TEST_ERC20_CONTRACT_ADDRESS => indexmap!(
                        // Give the accounts some balance.
                        account_balance_key => *ACCOUNT_INITIAL_BALANCE,
                        new_account_balance_key => *ACCOUNT_INITIAL_BALANCE,
                        // Give the first account mint permission (what is this?).
                        minter_var_address => *ACCOUNT_ADDRESS.0.key()
                    ),
                ),
                declared_classes: indexmap!(
                    // The class is not used in the execution, so it can be default.
                    class_hash0 => CompiledClassHash::default()
                ),
                deprecated_declared_classes: vec![
                    *TEST_ERC20_CONTRACT_CLASS_HASH,
                    class_hash1,
                    *ACCOUNT_CLASS_HASH,
                ],
                nonces: indexmap!(
                    *TEST_ERC20_CONTRACT_ADDRESS => Nonce::default(),
                    *CONTRACT_ADDRESS => Nonce::default(),
                    *DEPRECATED_CONTRACT_ADDRESS => Nonce::default(),
                    *ACCOUNT_ADDRESS => Nonce::default(),
                ),
            },
        )
        .unwrap()
        .append_classes(
            BlockNumber(0),
            &[(class_hash0, &SierraContractClass::default())],
            &[
                (*TEST_ERC20_CONTRACT_CLASS_HASH, &get_test_erc20_fee_contract_class()),
                (class_hash1, &get_test_deprecated_contract_class()),
                (*ACCOUNT_CLASS_HASH, &get_test_account_class()),
            ],
        )
        .unwrap()
        .append_casm(&class_hash0, &get_test_casm())
        .unwrap()
        .append_header(
            BlockNumber(1),
            &BlockHeader {
                block_hash: BlockHash(felt!(1_u128)),
                block_header_without_hash: BlockHeaderWithoutHash {
                    l1_gas_price: *GAS_PRICE,
                    sequencer: *SEQUENCER_ADDRESS,
                    timestamp: *BLOCK_TIMESTAMP,
                    parent_hash: BlockHash(felt!(0_u128)),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .unwrap()
        .append_body(BlockNumber(1), BlockBody::default())
        .unwrap()
        .append_state_diff(BlockNumber(1), ThinStateDiff::default())
        .unwrap()
        .append_classes(BlockNumber(1), &[], &[])
        .unwrap()
        .commit()
        .unwrap();
}

pub fn execute_simulate_transactions(
    storage_reader: StorageReader,
    maybe_pending_data: Option<PendingData>,
    txs: Vec<ExecutableTransactionInput>,
    tx_hashes: Option<Vec<TransactionHash>>,
    charge_fee: bool,
    validate: bool,
) -> Vec<TransactionSimulationOutput> {
    let chain_id = ChainId::Other(CHAIN_ID.to_string());

    simulate_transactions(
        txs,
        tx_hashes,
        &chain_id,
        storage_reader,
        maybe_pending_data,
        StateNumber::unchecked_right_after_block(BlockNumber(0)),
        BlockNumber(1),
        &get_test_execution_config(),
        charge_fee,
        validate,
        // TODO(DanB): Consider testing without overriding DA (It's already tested in the RPC)
        true,
        None,
    )
    .unwrap()
}

// Creates transactions for testing while resolving nonces and class hashes uniqueness.
pub struct TxsScenarioBuilder {
    // Each transaction by the same sender needs a unique nonce.
    sender_to_nonce: HashMap<ContractAddress, u128>,
    // Each declare class needs a unique class hash.
    next_class_hash: u128,
    // the result.
    txs: Vec<ExecutableTransactionInput>,
}

impl Default for TxsScenarioBuilder {
    fn default() -> Self {
        Self { sender_to_nonce: HashMap::new(), next_class_hash: 100_u128, txs: Vec::new() }
    }
}

impl TxsScenarioBuilder {
    pub fn collect(&self) -> Vec<ExecutableTransactionInput> {
        self.txs.clone()
    }

    pub fn invoke_deprecated(
        mut self,
        sender_address: ContractAddress,
        contract_address: ContractAddress,
        nonce: Option<Nonce>,
        only_query: OnlyQuery,
    ) -> Self {
        let calldata = calldata![
            *contract_address.0.key(),             // Contract address.
            selector_from_name("return_result").0, // EP selector.
            felt!(1_u8),                           // Calldata length.
            felt!(2_u8)                            // Calldata: num.
        ];
        let nonce = match nonce {
            None => self.next_nonce(sender_address),
            Some(nonce) => {
                let override_next_nonce: u128 =
                    u64::try_from(nonce.0.to_biguint()).expect("Nonce should fit in u64.").into();
                self.sender_to_nonce.insert(sender_address, override_next_nonce + 1);
                nonce
            }
        };
        let tx = ExecutableTransactionInput::Invoke(
            InvokeTransaction::V1(InvokeTransactionV1 {
                calldata,
                max_fee: *MAX_FEE,
                sender_address,
                nonce,
                ..Default::default()
            }),
            only_query,
        );
        self.txs.push(tx);
        self
    }

    pub fn declare_deprecated_class(mut self, sender_address: ContractAddress) -> Self {
        let tx = ExecutableTransactionInput::DeclareV1(
            DeclareTransactionV0V1 {
                max_fee: *MAX_FEE,
                sender_address,
                nonce: self.next_nonce(sender_address),
                class_hash: self.next_class_hash(),
                ..Default::default()
            },
            get_test_deprecated_contract_class(),
            0,
            false,
        );
        self.txs.push(tx);
        self
    }

    pub fn declare_class(mut self, sender_address: ContractAddress) -> TxsScenarioBuilder {
        let tx = ExecutableTransactionInput::DeclareV2(
            DeclareTransactionV2 {
                max_fee: *MAX_FEE,
                sender_address,
                nonce: self.next_nonce(sender_address),
                class_hash: self.next_class_hash(),
                ..Default::default()
            },
            get_test_casm(),
            DUMMY_SIERRA_SIZE,
            0,
            false,
            SierraVersion::LATEST,
        );
        self.txs.push(tx);
        self
    }

    pub fn deploy_account(mut self) -> TxsScenarioBuilder {
        let tx = ExecutableTransactionInput::DeployAccount(
            DeployAccountTransaction::V1(DeployAccountTransactionV1 {
                max_fee: *MAX_FEE,
                nonce: nonce!(0_u128),
                class_hash: *ACCOUNT_CLASS_HASH,
                ..Default::default()
            }),
            false,
        );
        self.txs.push(tx);
        self
    }

    // TODO(yair): add l1 handler transaction.

    fn next_nonce(&mut self, sender_address: ContractAddress) -> Nonce {
        match self.sender_to_nonce.get_mut(&sender_address) {
            Some(current) => {
                let res = nonce!(*current);
                *current += 1;
                res
            }
            None => {
                self.sender_to_nonce.insert(sender_address, 1);
                nonce!(0_u128)
            }
        }
    }

    fn next_class_hash(&mut self) -> ClassHash {
        let class_hash = ClassHash(self.next_class_hash.into());
        self.next_class_hash += 1;
        class_hash
    }
}
