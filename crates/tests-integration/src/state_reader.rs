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
    // Declare and deploy ERC20 contracts.
    let erc20_class_hash = erc20_contract.class_hash();
    let mut deployed_contracts = indexmap! {
        chain_info.fee_token_address(&FeeType::Eth) => erc20_class_hash,
        chain_info.fee_token_address(&FeeType::Strk) => erc20_class_hash
    };
    let mut deprecated_declared_classes = Vec::from([erc20_class_hash]);

    let mut storage_diffs = IndexMap::new();
    let mut declared_classes = IndexMap::new();

    // TODO: will soon be extracted to a function, kept like this for easier diff.
    let mut declare = |account: &FeatureAccount| match account.cairo_version() {
        CairoVersion::Cairo0 => {
            deprecated_declared_classes.push(account.class_hash());
        }
        CairoVersion::Cairo1 => {
            declared_classes.insert(account.class_hash(), Default::default());
        }
    };

    // TODO: will soon be extracted to a function, kept like this for easier diff.
    let mut deploy = |account: &FeatureAccount| {
        deployed_contracts.insert(account.sender_address, account.class_hash())
    };
    // TODO: will soon be extracted to a function, kept like this for easier diff.
    let mut fund = |account| fund_feature_account_contract(&mut storage_diffs, account, chain_info);

    // Inject deploy accounts into state. Once we stop doing this it'll only declare and fund, or
    // even only declare in case we want to fund via transfer.
    for account in test_defined_accounts {
        declare(account);
        deploy(account);
        fund(account);
    }

    for contract in default_test_contracts.iter().chain([erc20_contract]) {
        declare(contract);
        deploy(contract);
    }

    ThinStateDiff {
        storage_diffs,
        deployed_contracts,
        declared_classes,
        deprecated_declared_classes,
        ..Default::default()
    }
}

fn prepare_compiled_contract_classes(
    contract_classes_to_retrieve: impl Iterator<Item = FeatureAccount>,
) -> ContractClassesMap {
    let mut cairo0_contract_classes = Vec::new();
    let mut cairo1_contract_classes = Vec::new();
    for account in contract_classes_to_retrieve {
        match account.account.cairo_version() {
            CairoVersion::Cairo0 => {
                cairo0_contract_classes.push((
                    account.class_hash(),
                    serde_json::from_str(&account.account.get_raw_class()).unwrap(),
                ));
            }
            CairoVersion::Cairo1 => {
                cairo1_contract_classes.push((
                    account.class_hash(),
                    serde_json::from_str(&account.account.get_raw_class()).unwrap(),
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

fn fund_feature_account_contract(
    storage_diffs: &mut IndexMap<ContractAddress, IndexMap<StorageKey, Felt>>,
    account: &FeatureAccount,
    chain_info: &ChainInfo,
) {
    assert_matches!(
        account.account,
        FeatureContract::AccountWithLongValidate(_)
            | FeatureContract::AccountWithoutValidations(_)
            | FeatureContract::FaultyAccount(_),
        "Only Accounts can be funded, {account:?} is not an account",
    );

    fund_account(storage_diffs, &account.sender_address, chain_info);
}

fn fund_account(
    storage_diffs: &mut IndexMap<ContractAddress, IndexMap<StorageKey, Felt>>,
    account_address: &ContractAddress,
    chain_info: &ChainInfo,
) {
    let key_value = indexmap! {
        get_fee_token_var_address(*account_address) => felt!(BALANCE),
    };
    for fee_type in FeeType::iter() {
        storage_diffs
            .entry(chain_info.fee_token_address(&fee_type))
            .or_default()
            .extend(key_value.clone());
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
