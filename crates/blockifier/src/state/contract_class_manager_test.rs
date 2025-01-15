use std::sync::mpsc::{sync_channel, TrySendError};
use std::sync::Arc;

use assert_matches::assert_matches;
use rstest::rstest;
use starknet_sierra_multicompile::command_line_compiler::CommandLineCompiler;
use starknet_sierra_multicompile::config::SierraCompilationConfig;

use crate::blockifier::config::{CairoNativeRunConfig, ContractClassManagerConfig};
use crate::execution::contract_class::{CompiledClassV1, RunnableCompiledClass};
use crate::state::contract_class_manager::{
    process_compilation_request,
    CompilationRequest,
    ContractClassManager,
    ContractClassManagerError,
};
use crate::state::global_cache::{
    CachedCairoNative,
    ContractCaches,
    GLOBAL_CONTRACT_CACHE_SIZE_FOR_TEST,
};
use crate::test_utils::contracts::FeatureContract;
use crate::test_utils::{CairoVersion, RunnableCairo1};

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

fn create_faulty_request() -> CompilationRequest {
    let (class_hash, sierra, casm) = create_test_request();
    let mut sierra = sierra.as_ref().clone();

    // Truncate the sierra program to trigger an error.
    sierra.sierra_program = sierra.sierra_program[..100].to_vec();
    (class_hash, Arc::new(sierra), casm)
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


#[rstest]
#[case::run_native_while_waiting(true, true)]
#[case::run_native_without_waiting(true, false)]
#[should_panic(expected = "Native compilation is disabled.")]
#[case::run_without_native(false, false)]
fn test_send_compilation_request(
    #[case] run_cairo_native: bool,
    #[case] wait_on_native_compilation: bool,
) {
    let native_config =
    CairoNativeRunConfig { run_cairo_native, wait_on_native_compilation, ..Default::default() };
    let config =
    ContractClassManagerConfig { cairo_native_run_config: native_config, ..Default::default() };
    let manager = ContractClassManager::start(config);
    let request = create_test_request();
    let class_hash = request.0;
    let res = manager.send_compilation_request(request);
    assert!(res.is_ok(), "Compilation request failed with the follow error {}.", res.unwrap_err());
    if wait_on_native_compilation {
        assert_matches!(manager.get_native(&class_hash).unwrap(), CachedCairoNative::Compiled(_));
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
    manager.send_compilation_request(request).unwrap();
}

#[test]
fn test_send_compilation_request_channel_full() {
    let native_config = CairoNativeRunConfig {
        run_cairo_native: true,
        wait_on_native_compilation: false,
        channel_size: 0,
    };
    let manager = ContractClassManager::create_for_testing(native_config);
    let request = create_test_request();
    assert!(manager.sender.is_some(), "Sender should be Some");

    // Fill the channel (it can only hold 1 message)
    manager.send_compilation_request(request.clone()).unwrap();
    // Should log an error without panicking
    let result = manager.send_compilation_request(request.clone());
    assert!(result.is_err(), "Compilation request should not be sent successfully.");

    assert_eq!(
        result.unwrap_err(),
        ContractClassManagerError::TrySendError(TrySendError::Full(request))
    );
}

#[rstest]
#[case::success(create_test_request(), true)]
#[case::failure(create_faulty_request(), false)]
fn test_process_compilation_request(
    #[case] request: CompilationRequest,
    #[case] should_pass: bool,
) {
    use starknet_sierra_multicompile::errors::CompilationUtilError;

    let native_config = CairoNativeRunConfig {
        run_cairo_native: true,
        channel_size: TEST_CHANNEL_SIZE,
        wait_on_native_compilation: true,
    };
    let manager = ContractClassManager::create_for_testing(native_config);
    let compiler_config = SierraCompilationConfig::default();
    let compiler = Arc::new(CommandLineCompiler::new(compiler_config));
    let res = process_compilation_request(
        manager.contract_caches.clone(),
        compiler.clone(),
        request.clone(),
    );

    if should_pass {
        assert!(res.is_ok(), "Compilation failed with error{}.", res.unwrap_err());
        assert_matches!(manager.get_native(&request.0).unwrap(), CachedCairoNative::Compiled(_));
    } else {
        assert_matches!(
            res.unwrap_err(),
            ContractClassManagerError::CompilationError(CompilationUtilError::CompilationError(_))
        );
        assert_matches!(
            manager.get_native(&request.0).unwrap(),
            CachedCairoNative::CompilationFailed
        );
    }
}
