use std::sync::mpsc::sync_channel;
use std::sync::Arc;

use rstest::rstest;
use starknet_sierra_multicompile::command_line_compiler::CommandLineCompiler;
use starknet_sierra_multicompile::config::SierraCompilationConfig;

use crate::blockifier::config::{CairoNativeRunConfig, ContractClassManagerConfig};
use crate::execution::contract_class::RunnableCompiledClass;
use crate::execution::native::contract_class::NativeCompiledClassV1;
use crate::state::contract_class_manager::{
    process_compilation_request,
    run_compilation_worker,
    CompilationRequest,
    ContractClassManager,
};
use crate::state::global_cache::{
    CachedCairoNative,
    ContractCaches,
    GLOBAL_CONTRACT_CACHE_SIZE_FOR_TEST,
};
use crate::test_utils::contracts::FeatureContract;
use crate::test_utils::{CairoVersion, RunnableCairo1};

type TestRequestWithNative = (CompilationRequest, NativeCompiledClassV1);

const TEST_CHANNEL_SIZE: usize = 10;

fn create_faulty_request() -> TestRequestWithNative {
    let ((class_hash, sierra, casm), native) = create_test_request_with_native();
    let mut sierra = sierra.as_ref().clone();

    // Truncate the sierra program to trigger an error.
    sierra.sierra_program = sierra.sierra_program[..100].to_vec();

    let request = (class_hash, Arc::new(sierra), casm);

    (request, native)
}

fn create_test_request_from_contract(test_contract: FeatureContract) -> CompilationRequest {
    let class_hash = test_contract.get_class_hash();
    let sierra = Arc::new(test_contract.get_sierra());
    let casm = test_contract.get_casm();

    (class_hash, sierra, casm)
}

fn create_test_request() -> CompilationRequest {
    // Question (AvivG): are we interested in testing other contracts?
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Native));
    create_test_request_from_contract(test_contract)
}

fn get_native(test_contract: FeatureContract) -> NativeCompiledClassV1 {
    match test_contract.get_runnable_class() {
        RunnableCompiledClass::V1Native(native) => native,
        _ => panic!("Expected NativeCompiledClassV1"),
    }
}

fn create_test_request_with_native() -> TestRequestWithNative {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Native));
    let request = create_test_request_from_contract(test_contract);
    let native = get_native(test_contract);

    (request, native)
}

fn create_test_manager(native_config: CairoNativeRunConfig) -> ContractClassManager {
    let config =
        ContractClassManagerConfig { cairo_native_run_config: native_config, ..Default::default() };
    ContractClassManager::start(config)
}

#[rstest]
#[case::run_native_while_waiting(true, true)]
#[case::run_native_without_waiting(true, false)]
#[case::run_without_native(false, false)]
fn test_start(#[case] run_cairo_native: bool, #[case] wait_on_native_compilation: bool) {
    let native_config =
        CairoNativeRunConfig { run_cairo_native, wait_on_native_compilation, ..Default::default() };
    let manager = create_test_manager(native_config);

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
    let native_config = CairoNativeRunConfig { run_cairo_native: true, ..Default::default() };
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

#[test]
fn test_send_compilation_request_wait_on_native() {
    let native_config = CairoNativeRunConfig {
        run_cairo_native: true,
        wait_on_native_compilation: true,
        ..Default::default()
    };
    let manager = create_test_manager(native_config);
    let (request, expected_native) = create_test_request_with_native();
    // Extract class_hash before moving the request to the manager
    let class_hash = request.0;
    manager.send_compilation_request(request);

    assert_eq!(
        manager.get_native(&class_hash).unwrap(),
        CachedCairoNative::Compiled(expected_native),
        "Cached Native class should match the expected."
    );
}

#[test]
fn test_send_compilation_request_channel_full() {
    let native_config = CairoNativeRunConfig {
        run_cairo_native: true,
        wait_on_native_compilation: true,
        ..Default::default()
    };
    let manager = create_test_manager(native_config);
    let request = create_test_request();
    let second_request = create_test_request();

    // Fill the channel (it can only hold 1 message)
    manager.send_compilation_request(request);
    // Should log an error without panicking
    manager.send_compilation_request(second_request);
}

#[test]
#[should_panic(expected = "Native compilation is disabled.")]
fn test_send_compilation_request_run_cairo_native_false() {
    let native_config = CairoNativeRunConfig {
        run_cairo_native: false,
        wait_on_native_compilation: true,
        ..Default::default()
    };
    let manager = create_test_manager(native_config);
    let request = create_test_request();
    manager.send_compilation_request(request);
}

#[rstest]
#[case::success(create_test_request_with_native(), CachedCairoNative::Compiled(create_test_request_with_native().1))]
#[case::failure(create_faulty_request(), CachedCairoNative::CompilationFailed)]
fn test_process_compilation_request(
    #[case] request_w_native: TestRequestWithNative,
    #[case] expected_cached_native: CachedCairoNative,
) {
    let native_config = CairoNativeRunConfig {
        run_cairo_native: true,
        channel_size: TEST_CHANNEL_SIZE,
        wait_on_native_compilation: true,
    };
    let manager = create_test_manager(native_config);
    let (request, _native) = request_w_native;
    let compiler_config = SierraCompilationConfig::default();
    let compiler = Arc::new(CommandLineCompiler::new(compiler_config));
    process_compilation_request(manager.contract_caches.clone(), compiler.clone(), request.clone());

    assert_eq!(
        manager.get_native(&request.0).unwrap(),
        expected_cached_native,
        "Cached Native class should match the expected."
    );
}

#[rstest]
fn test_run_compilation_worker() {
    let native_config = CairoNativeRunConfig { run_cairo_native: true, ..Default::default() };
    let contract_caches = ContractCaches::new(GLOBAL_CONTRACT_CACHE_SIZE_FOR_TEST);
    let compiler_config = SierraCompilationConfig::default();
    let compiler = Arc::new(CommandLineCompiler::new(compiler_config));
    let (sender, receiver) = sync_channel(native_config.channel_size);
    let (request, expected_native) = create_test_request_with_native();

    sender.try_send(request.clone()).unwrap();
    // Drop the sender to close the channel and signal no more requests.
    drop(sender);
    // Since the channel is closed, the compilation worker should terminate after processing all
    // pending requests.
    run_compilation_worker(contract_caches.clone(), receiver, compiler);

    assert_eq!(
        contract_caches.get_native(&request.0).unwrap(),
        CachedCairoNative::Compiled(expected_native),
        "Cached Native class should match the expected."
    );
}
