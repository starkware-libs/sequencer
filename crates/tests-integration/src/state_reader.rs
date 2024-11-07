use std::net::SocketAddr;
use std::sync::Arc;

use assert_matches::assert_matches;
use blockifier::abi::abi_utils::get_fee_token_var_address;
use blockifier::context::{BlockContext, ChainInfo};
use blockifier::test_utils::contracts::FeatureContract;
use blockifier::test_utils::{
    CairoVersion,
    BALANCE,
    CURRENT_BLOCK_TIMESTAMP,
    DEFAULT_ETH_L1_GAS_PRICE,
    DEFAULT_STRK_L1_GAS_PRICE,
    TEST_SEQUENCER_ADDRESS,
};
use blockifier::transaction::objects::FeeType;
use blockifier::versioned_constants::VersionedConstants;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use indexmap::IndexMap;
use mempool_test_utils::starknet_api_test_utils::Contract;
use papyrus_common::pending_classes::PendingClasses;
use papyrus_rpc::{run_server, RpcConfig};
use papyrus_storage::body::BodyStorageWriter;
use papyrus_storage::class::ClassStorageWriter;
use papyrus_storage::compiled_class::CasmStorageWriter;
use papyrus_storage::header::HeaderStorageWriter;
use papyrus_storage::state::StateStorageWriter;
use papyrus_storage::test_utils::{get_test_storage, get_test_storage_with_config_by_scope};
use papyrus_storage::{StorageConfig, StorageReader, StorageWriter};
use starknet_api::block::{
    BlockBody,
    BlockHeader,
    BlockHeaderWithoutHash,
    BlockNumber,
    BlockTimestamp,
    GasPricePerToken,
};
use starknet_api::core::{ChainId, ClassHash, ContractAddress, Nonce, SequencerContractAddress};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::state::{StorageKey, ThinStateDiff};
use starknet_api::transaction::Fee;
use starknet_api::{contract_address, felt};
use starknet_client::reader::PendingData;
use starknet_types_core::felt::Felt;
use strum::IntoEnumIterator;
use tempfile::TempDir;
use tokio::sync::RwLock;

use crate::integration_test_utils::get_available_socket;

type ContractClassesMap =
    (Vec<(ClassHash, DeprecatedContractClass)>, Vec<(ClassHash, CasmContractClass)>);

pub struct StorageTestSetup {
    pub chain_id: ChainId,
    pub rpc_storage_reader: StorageReader,
    pub rpc_storage_handle: TempDir,
    pub batcher_storage_config: StorageConfig,
    pub batcher_storage_handle: TempDir,
}

impl StorageTestSetup {
    pub fn new(test_defined_accounts: Vec<Contract>) -> Self {
        let ((rpc_storage_reader, mut rpc_storage_writer), rpc_storage_file_handle) =
            get_test_storage();
        create_test_state(&mut rpc_storage_writer, test_defined_accounts.clone());
        let ((_, mut batcher_storage_writer), batcher_storage_config, batcher_storage_file_handle) =
            get_test_storage_with_config_by_scope(papyrus_storage::StorageScope::StateOnly);
        create_test_state(&mut batcher_storage_writer, test_defined_accounts);
        Self {
            chain_id: batcher_storage_config.db_config.chain_id.clone(),
            rpc_storage_reader,
            rpc_storage_handle: rpc_storage_file_handle,
            batcher_storage_config,
            batcher_storage_handle: batcher_storage_file_handle,
        }
    }
}

/// A variable number of identical accounts and test contracts are initialized and funded.
fn create_test_state(storage_writer: &mut StorageWriter, test_defined_accounts: Vec<Contract>) {
    let block_context = BlockContext::create_for_testing();

    let into_contract = |contract: FeatureContract| Contract {
        contract,
        sender_address: contract.get_instance_address(0),
    };
    let default_test_contracts = [
        FeatureContract::TestContract(CairoVersion::Cairo0),
        FeatureContract::TestContract(CairoVersion::Cairo1),
    ]
    .into_iter()
    .map(into_contract)
    .collect();

    let erc20_contract = FeatureContract::ERC20(CairoVersion::Cairo0);
    let erc20_contract = into_contract(erc20_contract);

    initialize_papyrus_test_state(
        storage_writer,
        block_context.chain_info(),
        test_defined_accounts,
        default_test_contracts,
        erc20_contract,
    );
}

fn initialize_papyrus_test_state(
    storage_writer: &mut StorageWriter,
    chain_info: &ChainInfo,
    test_defined_accounts: Vec<Contract>,
    default_test_contracts: Vec<Contract>,
    erc20_contract: Contract,
) {
    let state_diff = prepare_state_diff(
        chain_info,
        &test_defined_accounts,
        &default_test_contracts,
        &erc20_contract,
    );

    let contract_classes_to_retrieve =
        test_defined_accounts.into_iter().chain(default_test_contracts).chain([erc20_contract]);
    let (cairo0_contract_classes, cairo1_contract_classes) =
        prepare_compiled_contract_classes(contract_classes_to_retrieve);

    write_state_to_papyrus_storage(
        storage_writer,
        state_diff,
        &cairo0_contract_classes,
        &cairo1_contract_classes,
    )
}

fn prepare_state_diff(
    chain_info: &ChainInfo,
    test_defined_accounts: &[Contract],
    default_test_contracts: &[Contract],
    erc20_contract: &Contract,
) -> ThinStateDiff {
    let mut state_diff_builder = ThinStateDiffBuilder::new(chain_info);

    // Setup the common test contracts that are used by default in all test invokes.
    // TODO(batcher): this does nothing until we actually start excuting stuff in the batcher.
    state_diff_builder.set_contracts(default_test_contracts).declare().deploy();

    // Declare and deploy and the ERC20 contract, so that transfers from it can be made.
    state_diff_builder.set_contracts(std::slice::from_ref(erc20_contract)).declare().deploy();

    // TODO(deploy_account_support): once we have batcher with execution, replace with:
    // ```
    // state_diff_builder.set_contracts(accounts_defined_in_the_test).declare().fund();
    // ```
    // or use declare txs and transfers for both.
    state_diff_builder.inject_accounts_into_state(test_defined_accounts);

    state_diff_builder.build()
}

fn prepare_compiled_contract_classes(
    contract_classes_to_retrieve: impl Iterator<Item = Contract>,
) -> ContractClassesMap {
    let mut cairo0_contract_classes = Vec::new();
    let mut cairo1_contract_classes = Vec::new();
    for contract in contract_classes_to_retrieve {
        match contract.cairo_version() {
            CairoVersion::Cairo0 => {
                cairo0_contract_classes.push((
                    contract.class_hash(),
                    serde_json::from_str(&contract.raw_class()).unwrap(),
                ));
            }
            // todo(rdr): including both Cairo1 and Native versions for now. Temporal solution to
            // avoid compilation errors when using the "cairo_native" feature
            _ => {
                cairo1_contract_classes.push((
                    contract.class_hash(),
                    serde_json::from_str(&contract.raw_class()).unwrap(),
                ));
            }
        }
    }

    (cairo0_contract_classes, cairo1_contract_classes)
}

fn write_state_to_papyrus_storage(
    storage_writer: &mut StorageWriter,
    state_diff: ThinStateDiff,
    cairo0_contract_classes: &[(ClassHash, DeprecatedContractClass)],
    cairo1_contract_classes: &[(ClassHash, CasmContractClass)],
) {
    let block_number = BlockNumber(0);
    let block_header = test_block_header(block_number);
    let cairo0_contract_classes: Vec<_> =
        cairo0_contract_classes.iter().map(|(hash, contract)| (*hash, contract)).collect();

    let mut write_txn = storage_writer.begin_rw_txn().unwrap();

    for (class_hash, casm) in cairo1_contract_classes {
        write_txn = write_txn.append_casm(class_hash, casm).unwrap();
    }
    write_txn
        .append_header(block_number, &block_header)
        .unwrap()
        .append_body(block_number, BlockBody::default())
        .unwrap()
        .append_state_diff(block_number, state_diff)
        .unwrap()
        .append_classes(block_number, &[], &cairo0_contract_classes)
        .unwrap()
        .commit()
        .unwrap();
}

fn test_block_header(block_number: BlockNumber) -> BlockHeader {
    BlockHeader {
        block_header_without_hash: BlockHeaderWithoutHash {
            block_number,
            sequencer: SequencerContractAddress(contract_address!(TEST_SEQUENCER_ADDRESS)),
            l1_gas_price: GasPricePerToken {
                price_in_wei: DEFAULT_ETH_L1_GAS_PRICE.into(),
                price_in_fri: DEFAULT_STRK_L1_GAS_PRICE.into(),
            },
            l1_data_gas_price: GasPricePerToken {
                price_in_wei: DEFAULT_ETH_L1_GAS_PRICE.into(),
                price_in_fri: DEFAULT_STRK_L1_GAS_PRICE.into(),
            },
            l2_gas_price: GasPricePerToken {
                price_in_wei: VersionedConstants::latest_constants()
                    .convert_l1_to_l2_gas_price_round_up(DEFAULT_ETH_L1_GAS_PRICE.into()),
                price_in_fri: VersionedConstants::latest_constants()
                    .convert_l1_to_l2_gas_price_round_up(DEFAULT_STRK_L1_GAS_PRICE.into()),
            },
            timestamp: BlockTimestamp(CURRENT_BLOCK_TIMESTAMP),
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Spawns a papyrus rpc server for given state reader.
/// Returns the address of the rpc server.
pub async fn spawn_test_rpc_state_reader(
    storage_reader: StorageReader,
    chain_id: ChainId,
) -> SocketAddr {
    let rpc_config = RpcConfig {
        chain_id,
        server_address: get_available_socket().await.to_string(),
        ..Default::default()
    };
    let (addr, handle) = run_server(
        &rpc_config,
        Arc::new(RwLock::new(None)),
        Arc::new(RwLock::new(PendingData::default())),
        Arc::new(RwLock::new(PendingClasses::default())),
        storage_reader,
        "NODE VERSION",
    )
    .await
    .unwrap();
    // Spawn the server handle to keep the server running, otherwise the server will stop once the
    // handler is out of scope.
    tokio::spawn(handle.stopped());
    addr
}

/// Constructs a thin state diff from lists of contracts, where each contract can be declared,
/// deployed, and in case it is an account, funded.
#[derive(Default)]
struct ThinStateDiffBuilder<'a> {
    contracts: &'a [Contract],
    deprecated_declared_classes: Vec<ClassHash>,
    declared_classes: IndexMap<ClassHash, starknet_api::core::CompiledClassHash>,
    deployed_contracts: IndexMap<ContractAddress, ClassHash>,
    storage_diffs: IndexMap<ContractAddress, IndexMap<StorageKey, Felt>>,
    // TODO(deploy_account_support): delete field once we have batcher with execution.
    nonces: IndexMap<ContractAddress, Nonce>,
    chain_info: ChainInfo,
    initial_account_balance: Felt,
}

impl<'a> ThinStateDiffBuilder<'a> {
    fn new(chain_info: &ChainInfo) -> Self {
        const TEST_INITIAL_ACCOUNT_BALANCE: Fee = BALANCE;
        let erc20 = FeatureContract::ERC20(CairoVersion::Cairo0);
        let erc20_class_hash = erc20.get_class_hash();

        let deployed_contracts: IndexMap<ContractAddress, ClassHash> = FeeType::iter()
            .map(|fee_type| (chain_info.fee_token_address(&fee_type), erc20_class_hash))
            .collect();

        Self {
            chain_info: chain_info.clone(),
            initial_account_balance: felt!(TEST_INITIAL_ACCOUNT_BALANCE.0),
            deployed_contracts,
            ..Default::default()
        }
    }

    fn set_contracts(&mut self, contracts: &'a [Contract]) -> &mut Self {
        self.contracts = contracts;
        self
    }

    fn declare(&mut self) -> &mut Self {
        for contract in self.contracts {
            match contract.cairo_version() {
                CairoVersion::Cairo0 => {
                    self.deprecated_declared_classes.push(contract.class_hash())
                }
                // todo(rdr): including both Cairo1 and Native versions for now. Temporal solution
                // to avoid compilation errors when using the "cairo_native" feature
                _ => {
                    self.declared_classes.insert(contract.class_hash(), Default::default());
                }
            }
        }
        self
    }

    fn deploy(&mut self) -> &mut Self {
        for contract in self.contracts {
            self.deployed_contracts.insert(contract.sender_address, contract.class_hash());
        }
        self
    }

    /// Only applies for contracts that are accounts, for non-accounts only declare and deploy work.
    fn fund(&mut self) -> &mut Self {
        for account in self.contracts {
            assert_matches!(
                account.contract,
                FeatureContract::AccountWithLongValidate(_)
                    | FeatureContract::AccountWithoutValidations(_)
                    | FeatureContract::FaultyAccount(_),
                "Only Accounts can be funded, {account:?} is not an account",
            );

            let fee_token_address = get_fee_token_var_address(account.sender_address);
            for fee_type in FeeType::iter() {
                self.storage_diffs
                    .entry(self.chain_info.fee_token_address(&fee_type))
                    .or_default()
                    .insert(fee_token_address, self.initial_account_balance);
            }
        }

        self
    }

    // TODO(deploy_account_support): delete method once we have batcher with execution.
    fn inject_accounts_into_state(&mut self, accounts_defined_in_the_test: &'a [Contract]) {
        self.set_contracts(accounts_defined_in_the_test).declare().deploy().fund();

        // Set nonces as 1 in the state so that subsequent invokes can pass validation.
        self.nonces = self
            .deployed_contracts
            .iter()
            .map(|(&address, _)| (address, Nonce(Felt::ONE)))
            .collect();
    }

    fn build(self) -> ThinStateDiff {
        ThinStateDiff {
            storage_diffs: self.storage_diffs,
            deployed_contracts: self.deployed_contracts,
            declared_classes: self.declared_classes,
            deprecated_declared_classes: self.deprecated_declared_classes,
            nonces: self.nonces,
            ..Default::default()
        }
    }
}
