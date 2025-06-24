use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};  
use blockifier_test_utils::contracts::FeatureContract;
use starknet_api::abi::abi_utils::get_fee_token_var_address;
use starknet_api::block::FeeType;
use starknet_api::core::ContractAddress;
use starknet_api::felt;
use starknet_api::transaction::fields::Fee;
use strum::IntoEnumIterator;

use crate::blockifier::config::ContractClassManagerConfig;
use crate::context::ChainInfo;
use crate::execution::contract_class::RunnableCompiledClass;
use crate::state::cached_state::CachedState;
use crate::state::contract_class_manager::ContractClassManager;
use crate::state::state_reader_and_contract_manager::StateReaderAndContractManager;
use crate::test_utils::contracts::FeatureContractData;
use crate::test_utils::dict_state_reader::DictStateReader;

/// Utility to fund an account.
pub fn fund_account(
    chain_info: &ChainInfo,
    account_address: ContractAddress,
    initial_balance: Fee,
    state_reader: &mut DictStateReader,
) {
    let storage_view = &mut state_reader.storage_view;
    let balance_key = get_fee_token_var_address(account_address);
    for fee_type in FeeType::iter() {
        storage_view.insert(
            (chain_info.fee_token_address(&fee_type), balance_key),
            felt!(initial_balance.0),
        );
    }
}

/// Sets up a state reader for testing:
/// * "Declares" a Cairo0 ERC20 contract (class hash => class mapping set).
/// * "Deploys" two ERC20 contracts (address => class hash mapping set) at the fee token addresses
///   on the input block context.
/// * "Declares" the input list of contracts.
/// * "Deploys" the requested number of instances of each input contract.
/// * Makes each input account contract privileged.
pub fn setup_test_state(
    chain_info: &ChainInfo,
    initial_balances: Fee,
    contract_instances: &[(FeatureContractData, u16)],
    erc20_contract_version: CairoVersion,
    state_reader: &mut DictStateReader,
) {
    // Declare and deploy account and ERC20 contracts.
    let erc20 = FeatureContract::ERC20(erc20_contract_version);
    let erc20_class_hash = erc20.get_class_hash();
    state_reader.add_class(&FeatureContractData::from(erc20));
    state_reader
        .address_to_class_hash
        .insert(chain_info.fee_token_address(&FeeType::Eth), erc20_class_hash);
    state_reader
        .address_to_class_hash
        .insert(chain_info.fee_token_address(&FeeType::Strk), erc20_class_hash);

    // Set up the rest of the requested contracts.
    for (contract, n_instances) in contract_instances.iter() {
        let class_hash = contract.class_hash;
        state_reader.add_class(contract);
        for instance in 0..*n_instances {
            let instance_address = contract.get_instance_address(instance);
            state_reader.address_to_class_hash.insert(instance_address, class_hash);
        }
    }

    // fund the accounts.
    for (contract, n_instances) in contract_instances.iter() {
        for instance in 0..*n_instances {
            let instance_address = contract.get_instance_address(instance);
            if contract.require_funding {
                fund_account(chain_info, instance_address, initial_balances, state_reader);
            }
        }
    }
}

pub fn test_state(
    chain_info: &ChainInfo,
    initial_balances: Fee,
    contract_instances: &[(FeatureContract, u16)],
) -> CachedState<DictStateReader> {
    let contract_instances_vec: Vec<(FeatureContractData, u16)> = contract_instances
        .iter()
        .map(|(feature_contract, i)| ((*feature_contract).into(), *i))
        .collect();
    test_state_ex(chain_info, initial_balances, &contract_instances_vec[..])
}

pub fn test_state_ex(
    chain_info: &ChainInfo,
    initial_balances: Fee,
    contract_instances: &[(FeatureContractData, u16)],
) -> CachedState<DictStateReader> {
    //Here we assume that the first contract in the list is the account contract. 
    //TODO(YonatanK): Find a way to avoid this assumption.
    let erc20_version = contract_instances
        .first()
        .map(|(contract_data, _)| match &contract_data.runnable_class {
            RunnableCompiledClass::V0(_) => CairoVersion::Cairo0,
            RunnableCompiledClass::V1(_) => CairoVersion::Cairo1(RunnableCairo1::Casm),
            #[cfg(feature = "cairo_native")]
            RunnableCompiledClass::V1Native(_) => CairoVersion::Cairo1(RunnableCairo1::Native),
        })
        .unwrap_or(CairoVersion::Cairo0);

    test_state_inner(chain_info, initial_balances, contract_instances, erc20_version)
}


pub fn test_state_with_contract_manager(
    chain_info: &ChainInfo,
    initial_balances: Fee,
    contract_instances: &[(FeatureContract, u16)],
) -> CachedState<StateReaderAndContractManager<DictStateReader>> {
    let contract_instances_vec: Vec<(FeatureContractData, u16)> = contract_instances
        .iter()
        .map(|(feature_contract, i)| ((*feature_contract).into(), *i))
        .collect();
    test_state_inner_with_contract_manager(
        chain_info,
        initial_balances,
        &contract_instances_vec[..],
        CairoVersion::Cairo0,
    )
}

pub fn test_state_inner(
    chain_info: &ChainInfo,
    initial_balances: Fee,
    contract_instances: &[(FeatureContractData, u16)],
    erc20_version: CairoVersion,
) -> CachedState<DictStateReader> {
    let mut reader = DictStateReader::default();
    setup_test_state(chain_info, initial_balances, contract_instances, erc20_version, &mut reader);
    CachedState::from(reader)
}

pub fn test_state_inner_with_contract_manager(
    chain_info: &ChainInfo,
    initial_balances: Fee,
    contract_instances: &[(FeatureContractData, u16)],
    erc20_version: CairoVersion,
) -> CachedState<StateReaderAndContractManager<DictStateReader>> {
    let mut reader = DictStateReader::default();
    setup_test_state(chain_info, initial_balances, contract_instances, erc20_version, &mut reader);

    #[cfg(not(feature = "cairo_native"))]
    let (run_cairo_native, wait_on_native_compilation) = (false, false);
    #[cfg(feature = "cairo_native")]
    let (run_cairo_native, wait_on_native_compilation) = (true, true);

    let manager = ContractClassManager::start(ContractClassManagerConfig::create_for_testing(
        run_cairo_native,
        wait_on_native_compilation,
    ));
    let reader = StateReaderAndContractManager {
        state_reader: reader.clone(),
        contract_class_manager: manager,
    };

    CachedState::from(reader)
}
