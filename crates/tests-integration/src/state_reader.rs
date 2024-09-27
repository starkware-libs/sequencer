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
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use indexmap::{indexmap, IndexMap};
use mempool_test_utils::starknet_api_test_utils::FeatureAccount;
use papyrus_common::pending_classes::PendingClasses;
use papyrus_rpc::{run_server, RpcConfig};
use papyrus_storage::body::BodyStorageWriter;
use papyrus_storage::class::ClassStorageWriter;
use papyrus_storage::compiled_class::CasmStorageWriter;
use papyrus_storage::header::HeaderStorageWriter;
use papyrus_storage::state::StateStorageWriter;
use papyrus_storage::test_utils::get_test_storage;
use papyrus_storage::StorageReader;
use starknet_api::block::{
    BlockBody,
    BlockHeader,
    BlockHeaderWithoutHash,
    BlockNumber,
    BlockTimestamp,
    GasPrice,
    GasPricePerToken,
};
use starknet_api::core::{ClassHash, ContractAddress, PatriciaKey, SequencerContractAddress};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::state::{StorageKey, ThinStateDiff};
use starknet_api::{contract_address, felt, patricia_key};
use starknet_client::reader::PendingData;
use starknet_types_core::felt::Felt;
use strum::IntoEnumIterator;
use tokio::sync::RwLock;

use crate::integration_test_utils::get_available_socket;

type ContractClassesMap =
    (Vec<(ClassHash, DeprecatedContractClass)>, Vec<(ClassHash, CasmContractClass)>);

/// StateReader for integration tests.
///
/// Creates a papyrus storage reader and spawns a papyrus rpc server for it.
/// Returns the address of the rpc server.
/// A variable number of identical accounts and test contracts are initialized and funded.
pub async fn spawn_test_rpc_state_reader(test_defined_accounts: Vec<FeatureAccount>) -> SocketAddr {
    let block_context = BlockContext::create_for_testing();

    let into_dummy_feature_account = |account: FeatureContract| FeatureAccount {
        account,
        sender_address: account.get_instance_address(0),
    };
    let default_test_contracts = [
        FeatureContract::TestContract(CairoVersion::Cairo0),
        FeatureContract::TestContract(CairoVersion::Cairo1),
    ]
    .into_iter()
    .map(into_dummy_feature_account)
    .collect();

    let erc20_contract = FeatureContract::ERC20(CairoVersion::Cairo0);
    let erc20_contract = into_dummy_feature_account(erc20_contract);

    let storage_reader = initialize_papyrus_test_state(
        block_context.chain_info(),
        test_defined_accounts,
        default_test_contracts,
        erc20_contract,
    );
    run_papyrus_rpc_server(storage_reader).await
}

fn initialize_papyrus_test_state(
    chain_info: &ChainInfo,
    test_defined_accounts: Vec<FeatureAccount>,
    default_test_contracts: Vec<FeatureAccount>,
    erc20_contract: FeatureAccount,
) -> StorageReader {
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

    write_state_to_papyrus_storage(state_diff, &cairo0_contract_classes, &cairo1_contract_classes)
}

fn prepare_state_diff(
    chain_info: &ChainInfo,
    test_defined_accounts: &[FeatureAccount],
    default_test_contracts: &[FeatureAccount],
    erc20_contract: &FeatureAccount,
) -> ThinStateDiff {
    let mut state_diff_builder = ThinStateDiffBuilder::new(chain_info);

    // Setup the common test contracts that are used by default in all test invokes.
    // TODO(batcher): this does nothing until we actually start excuting stuff in the batcher.
    state_diff_builder.set_accounts(default_test_contracts).declare().deploy();

    // Declare and deploy and the ERC20 contract, so that transfers from it can be made.
    state_diff_builder.set_accounts(std::slice::from_ref(erc20_contract)).declare().deploy();

    // TODO(deploy_account_support): once we have batcher with execution, replace with:
    // ```
    // state_diff_builder.set_contracts(accounts_defined_in_the_test).declare().fund();
    // ```
    // or use declare txs and transfers for both.
    state_diff_builder.inject_accounts_into_state(test_defined_accounts);

    state_diff_builder.build()
}

fn prepare_compiled_contract_classes(
    contract_classes_to_retrieve: impl Iterator<Item = FeatureAccount>,
) -> ContractClassesMap {
    let mut cairo0_contract_classes = Vec::new();
    let mut cairo1_contract_classes = Vec::new();
    for account in contract_classes_to_retrieve {
        match account.cairo_version() {
            CairoVersion::Cairo0 => {
                cairo0_contract_classes.push((
                    account.class_hash(),
                    serde_json::from_str(&account.raw_class()).unwrap(),
                ));
            }
            CairoVersion::Cairo1 => {
                cairo1_contract_classes.push((
                    account.class_hash(),
                    serde_json::from_str(&account.raw_class()).unwrap(),
                ));
            }
        }
    }

    (cairo0_contract_classes, cairo1_contract_classes)
}

fn write_state_to_papyrus_storage(
    state_diff: ThinStateDiff,
    cairo0_contract_classes: &[(ClassHash, DeprecatedContractClass)],
    cairo1_contract_classes: &[(ClassHash, CasmContractClass)],
) -> StorageReader {
    let block_number = BlockNumber(0);
    let block_header = test_block_header(block_number);
    let cairo0_contract_classes: Vec<_> =
        cairo0_contract_classes.iter().map(|(hash, contract)| (*hash, contract)).collect();

    let (storage_reader, mut storage_writer) = get_test_storage().0;
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

    storage_reader
}

fn test_block_header(block_number: BlockNumber) -> BlockHeader {
    BlockHeader {
        block_header_without_hash: BlockHeaderWithoutHash {
            block_number,
            sequencer: SequencerContractAddress(contract_address!(TEST_SEQUENCER_ADDRESS)),
            l1_gas_price: GasPricePerToken {
                price_in_wei: GasPrice(DEFAULT_ETH_L1_GAS_PRICE),
                price_in_fri: GasPrice(DEFAULT_STRK_L1_GAS_PRICE),
            },
            l1_data_gas_price: GasPricePerToken {
                price_in_wei: GasPrice(DEFAULT_ETH_L1_GAS_PRICE),
                price_in_fri: GasPrice(DEFAULT_STRK_L1_GAS_PRICE),
            },
            timestamp: BlockTimestamp(CURRENT_BLOCK_TIMESTAMP),
            ..Default::default()
        },
        ..Default::default()
    }
}

async fn run_papyrus_rpc_server(storage_reader: StorageReader) -> SocketAddr {
    let rpc_config = RpcConfig {
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

#[derive(Default)]
struct ThinStateDiffBuilder<'a> {
    accounts: &'a [FeatureAccount],
    deprecated_declared_classes: Vec<ClassHash>,
    declared_classes: IndexMap<ClassHash, starknet_api::core::CompiledClassHash>,
    deployed_contracts: IndexMap<ContractAddress, ClassHash>,
    storage_diffs: IndexMap<ContractAddress, IndexMap<StorageKey, Felt>>,
    chain_info: ChainInfo,
    initial_account_balance: Felt,
}

impl<'a> ThinStateDiffBuilder<'a> {
    fn new(chain_info: &ChainInfo) -> Self {
        const TEST_INITIAL_ACCOUNT_BALANCE: u128 = BALANCE;
        let erc20 = FeatureContract::ERC20(CairoVersion::Cairo0);
        let erc20_class_hash = erc20.get_class_hash();

        let deployed_contracts: IndexMap<ContractAddress, ClassHash> = FeeType::iter()
            .map(|fee_type| (chain_info.fee_token_address(&fee_type), erc20_class_hash))
            .collect();

        Self {
            chain_info: chain_info.clone(),
            initial_account_balance: felt!(TEST_INITIAL_ACCOUNT_BALANCE),
            deployed_contracts,
            ..Default::default()
        }
    }

    fn set_accounts(&mut self, accounts: &'a [FeatureAccount]) -> &mut Self {
        self.accounts = accounts;
        self
    }

    fn declare(&mut self) -> &mut Self {
        for account in self.accounts {
            match account.cairo_version() {
                CairoVersion::Cairo0 => self.deprecated_declared_classes.push(account.class_hash()),
                CairoVersion::Cairo1 => {
                    self.declared_classes.insert(account.class_hash(), Default::default());
                }
            }
        }
        self
    }

    fn deploy(&mut self) -> &mut Self {
        for account in self.accounts {
            self.deployed_contracts.insert(account.sender_address, account.class_hash());
        }
        self
    }

    fn fund(&mut self) -> &mut Self {
        for account in self.accounts {
            assert_matches!(
                account.account,
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
    fn inject_accounts_into_state(&mut self, accounts_defined_in_the_test: &'a [FeatureAccount]) {
        self.set_accounts(accounts_defined_in_the_test).declare().deploy().fund();
        todo!("bump nonce of account to 1, since we just injected it into state.")
    }

    fn build(self) -> ThinStateDiff {
        ThinStateDiff {
            storage_diffs: self.storage_diffs,
            deployed_contracts: self.deployed_contracts,
            declared_classes: self.declared_classes,
            deprecated_declared_classes: self.deprecated_declared_classes,
            ..Default::default()
        }
    }
}
