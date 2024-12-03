use pretty_assertions::assert_eq;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::core::calculate_contract_address;
use starknet_api::transaction::fields::{Calldata, ContractAddressSalt, Fee};
use starknet_api::{calldata, felt};
use test_case::test_case;

use crate::context::ChainInfo;
use crate::execution::call_info::CallExecution;
use crate::execution::entry_point::CallEntryPoint;
use crate::retdata;
use crate::state::state_api::StateReader;
use crate::test_utils::contracts::{FeatureContract, RunnableContractVersion};
use crate::test_utils::initial_test_state::test_state;
use crate::test_utils::{calldata_for_deploy_test, trivial_external_entry_point_new};

#[test_case(RunnableContractVersion::Cairo1Casm;"VM")]
#[cfg_attr(
    feature = "cairo_native",
    test_case(RunnableContractVersion::Cairo1Native; "Native")
)]
fn no_constructor(cairo_version: RunnableContractVersion) {
    // TODO(Yoni): share the init code of the tests in this file.
    let deployer_contract = FeatureContract::TestContract(cairo_version);
    let empty_contract = FeatureContract::Empty(RunnableContractVersion::Cairo1Casm);
    let class_hash = empty_contract.get_class_hash();

    let mut state = test_state(
        &ChainInfo::create_for_testing(),
        Fee(0),
        &[(deployer_contract, 1), (empty_contract, 0)],
    );

    let calldata = calldata_for_deploy_test(class_hash, &[], true);
    let entry_point_call = CallEntryPoint {
        entry_point_selector: selector_from_name("test_deploy"),
        calldata,
        ..trivial_external_entry_point_new(deployer_contract)
    };

    let deploy_call = &entry_point_call.execute_directly(&mut state).unwrap();
    assert_eq!(
        deploy_call.execution,
        CallExecution { retdata: retdata![], gas_consumed: 205200, ..CallExecution::default() }
    );

    let deployed_contract_address = calculate_contract_address(
        ContractAddressSalt::default(),
        class_hash,
        &calldata![],
        deployer_contract.get_instance_address(0),
    )
    .unwrap();

    let constructor_call = &deploy_call.inner_calls[0];

    assert_eq!(constructor_call.call.storage_address, deployed_contract_address);
    assert_eq!(
        constructor_call.execution,
        CallExecution { retdata: retdata![], gas_consumed: 0, ..CallExecution::default() }
    );
    assert_eq!(state.get_class_hash_at(deployed_contract_address).unwrap(), class_hash);
}

#[test_case(RunnableContractVersion::Cairo1Casm;"VM")]
#[cfg_attr(
    feature = "cairo_native",
    test_case(RunnableContractVersion::Cairo1Native; "Native")
)]
fn no_constructor_nonempty_calldata(cairo_version: RunnableContractVersion) {
    let deployer_contract = FeatureContract::TestContract(cairo_version);
    let empty_contract = FeatureContract::Empty(RunnableContractVersion::Cairo1Casm);
    let class_hash = empty_contract.get_class_hash();

    let mut state = test_state(
        &ChainInfo::create_for_testing(),
        Fee(0),
        &[(deployer_contract, 1), (empty_contract, 0)],
    );

    let calldata = calldata_for_deploy_test(class_hash, &[felt!(1_u8), felt!(1_u8)], true);

    let entry_point_call = CallEntryPoint {
        entry_point_selector: selector_from_name("test_deploy"),
        calldata,
        ..trivial_external_entry_point_new(deployer_contract)
    };

    let error = entry_point_call.execute_directly(&mut state).unwrap_err().to_string();
    assert!(error.contains(
        "Invalid input: constructor_calldata; Cannot pass calldata to a contract with no \
         constructor."
    ));
}

#[test_case(RunnableContractVersion::Cairo1Casm;"VM")]
#[cfg_attr(
    feature = "cairo_native",
    test_case(RunnableContractVersion::Cairo1Native; "Native")
)]
fn with_constructor(cairo_version: RunnableContractVersion) {
    let deployer_contract = FeatureContract::TestContract(cairo_version);
    let mut state = test_state(&ChainInfo::create_for_testing(), Fee(0), &[(deployer_contract, 1)]);

    let class_hash = deployer_contract.get_class_hash();
    let constructor_calldata = vec![
        felt!(1_u8), // Calldata: address.
        felt!(1_u8), // Calldata: value.
    ];

    let calldata = calldata_for_deploy_test(class_hash, &constructor_calldata, true);

    let entry_point_call = CallEntryPoint {
        entry_point_selector: selector_from_name("test_deploy"),
        calldata,
        ..trivial_external_entry_point_new(deployer_contract)
    };

    // No errors expected.
    let contract_address = calculate_contract_address(
        ContractAddressSalt::default(),
        class_hash,
        &Calldata(constructor_calldata.clone().into()),
        deployer_contract.get_instance_address(0),
    )
    .unwrap();

    let deploy_call = &entry_point_call.execute_directly(&mut state).unwrap();
    assert_eq!(
        deploy_call.execution,
        CallExecution { retdata: retdata![], gas_consumed: 214550, ..CallExecution::default() }
    );

    let constructor_call = &deploy_call.inner_calls[0];

    assert_eq!(constructor_call.call.storage_address, contract_address);
    assert_eq!(
        constructor_call.execution,
        CallExecution {
            // The test contract constructor returns its first argument.
            retdata: retdata![constructor_calldata[0]],
            // This reflects the gas cost of storage write syscall.
            gas_consumed: 4610,
            ..CallExecution::default()
        }
    );
    assert_eq!(state.get_class_hash_at(contract_address).unwrap(), class_hash);
}

#[test_case(RunnableContractVersion::Cairo1Casm;"VM")]
#[cfg_attr(
    feature = "cairo_native",
    test_case(RunnableContractVersion::Cairo1Native; "Native")
)]
fn to_unavailable_address(cairo_version: RunnableContractVersion) {
    let deployer_contract = FeatureContract::TestContract(cairo_version);
    let mut state = test_state(&ChainInfo::create_for_testing(), Fee(0), &[(deployer_contract, 1)]);

    let class_hash = deployer_contract.get_class_hash();
    let constructor_calldata = vec![
        felt!(1_u8), // Calldata: address.
        felt!(1_u8), // Calldata: value.
    ];

    let calldata = calldata_for_deploy_test(class_hash, &constructor_calldata, true);

    let entry_point_call = CallEntryPoint {
        entry_point_selector: selector_from_name("test_deploy"),
        calldata,
        ..trivial_external_entry_point_new(deployer_contract)
    };

    entry_point_call.clone().execute_directly(&mut state).unwrap();
    let error = entry_point_call.execute_directly(&mut state).unwrap_err().to_string();

    assert!(error.contains("Deployment failed:"));
}
