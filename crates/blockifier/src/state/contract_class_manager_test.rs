use std::sync::mpsc::sync_channel;
use std::sync::Arc;

use blockifier_test_utils::cairo_versions::RunnableCairo1;
use rstest::rstest;

use crate::blockifier::config::CairoNativeRunConfig;
use crate::execution::contract_class::{CompiledClassV1, RunnableCompiledClass};
use crate::state::contract_class_manager::{CompilationRequest, ContractClassManager};
use crate::state::global_cache::{ContractCaches, GLOBAL_CONTRACT_CACHE_SIZE_FOR_TEST};
use crate::test_utils::contracts::FeatureContract;
use crate::test_utils::CairoVersion;

const TEST_CHANNEL_SIZE: usize = 10;

fn get_casm(test_contract: FeatureContract) -> CompiledClassV1 {
    match test_contract.get_runnable_class() {
        RunnableCompiledClass::V1(casm) => casm,
        _ => panic!("Expected CompiledClassV1"),
    }
}

fn create_test_request_from_contract(test_contract: FeatureContract) -> CompilationRequest {
    let class_hash = test_contract.get_class_hash();
    let sierra = Arc::new(test_contract.get_sierra());
    let casm = get_casm(test_contract);

    (class_hash, sierra, casm)
}

fn create_test_request() -> CompilationRequest {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));
    create_test_request_from_contract(test_contract)
}

#[rstest]
#[case::run_native_while_waiting(true, true)]
#[case::run_native_without_waiting(true, false)]
#[case::run_without_native(false, false)]
fn test_start(#[case] run_cairo_native: bool, #[case] wait_on_native_compilation: bool) {
    let native_config =
        CairoNativeRunConfig { run_cairo_native, wait_on_native_compilation, ..Default::default() };
    let manager = ContractClassManager::create_for_testing(native_config);

    assert_eq!(manager.cairo_native_run_config, native_config);
    if run_cairo_native {
        if wait_on_native_compilation {
            assert!(
                manager.sender.is_none(),
                "Sender should be None - the compilation worker is not used."
            );
            assert!(
                manager.compiler.is_some(),
                "Compiler should be Some - compilation is not offloaded to the compilation worker."
            );
        } else {
            assert!(
                manager.sender.is_some(),
                "Sender should be Some - the compilation worker is used."
            );
            assert!(
                manager.compiler.is_none(),
                "Compiler should be None - compilation is offloaded to the compilation worker."
            );
        }
    } else {
        assert!(manager.sender.is_none(), "Sender should be None- Cairo native is disabled.");
        assert!(manager.compiler.is_none(), "Compiler should be None - Cairo native is disabled.");
    }
}

#[test]
#[should_panic(expected = "Compilation request channel is closed.")]
fn test_send_compilation_request_channel_disconnected() {
    // We use the channel to send native compilation requests.
    let native_config = CairoNativeRunConfig {
        run_cairo_native: true,
        wait_on_native_compilation: false,
        channel_size: TEST_CHANNEL_SIZE,
    };
    let (sender, receiver) = sync_channel(native_config.channel_size);
    let manager = ContractClassManager {
        cairo_native_run_config: native_config,
        contract_caches: ContractCaches::new(GLOBAL_CONTRACT_CACHE_SIZE_FOR_TEST),
        sender: Some(sender),
        compiler: None,
    };
    // Disconnect the channel by dropping the receiver.
    drop(receiver);

    // Sending request with a disconnected channel should panic.
    let request = create_test_request();
    manager.send_compilation_request(request);
}
