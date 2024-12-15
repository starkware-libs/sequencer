use std::sync::mpsc::sync_channel;
use std::sync::Arc;

use rstest::rstest;
use starknet_sierra_compile::command_line_compiler::CommandLineCompiler;
use starknet_sierra_compile::config::SierraCompilationConfig;

use crate::blockifier::config::{CairoNativeRunConfig, ContractClassManagerConfig};
use crate::execution::contract_class::RunnableCompiledClass;
use crate::execution::native::contract_class::NativeCompiledClassV1;
use crate::state::contract_class_manager::{
    process_compilation_request,
    run_compilation_worker,
    CompilationRequest,
    ContractClassManager,
};
use crate::state::global_cache::{CachedCairoNative, ContractCaches};
use crate::test_utils::contracts::FeatureContract;
use crate::test_utils::{CairoVersion, RunnableCairo1};

type TestRequestWithNative = (CompilationRequest, NativeCompiledClassV1);

const TEST_CHANNEL_SIZE: usize = 10;

#[rstest]
fn test_start(
    #[values(true, false)] run_cairo_native: bool,
    #[values(true, false)] wait_on_native_compilation: bool,
) {
    let native_config =
        CairoNativeRunConfig { run_cairo_native, wait_on_native_compilation, ..Default::default() };
    let config =
        ContractClassManagerConfig { cairo_native_run_config: native_config, ..Default::default() };
    let manager = ContractClassManager::start(config.clone());

    assert_eq!(manager.cairo_native_run_config, native_config);
    if !run_cairo_native | wait_on_native_compilation {
        assert!(manager.sender.is_none(), "Sender should be None");
    } else {
        assert!(manager.sender.is_some(), "Sender should be Some");
    }
    if !run_cairo_native | !wait_on_native_compilation {
        assert!(manager.compiler.is_none(), "Compiler should be None");
    } else {
        // TODO(AvivG): any constraints on initial compiler?
        assert!(manager.compiler.is_some(), "Compiler should be Some");
    }
    // TODO(AvivG): check if the compilation worker is spawned? by waiting on log
}

#[test]
#[should_panic]
fn test_send_compilation_request_channel_disconnected() {
    let native_config = CairoNativeRunConfig { run_cairo_native: true, ..Default::default() };
    let config =
        ContractClassManagerConfig { cairo_native_run_config: native_config, ..Default::default() };
    let contract_caches = ContractCaches::new(config.contract_cache_size);
    let (sender, receiver) = sync_channel(native_config.channel_size);
    drop(receiver);
    let manager = ContractClassManager {
        cairo_native_run_config: native_config,
        contract_caches,
        sender: Some(sender),
        compiler: None,
    };

    let request = create_test_request();
    // Sending request with a disconnected channel should panic.
    manager.send_compilation_request(request);
}

#[test]
fn test_send_compilation_request_wait_on_native() {
    let native_config = CairoNativeRunConfig {
        run_cairo_native: true,
        wait_on_native_compilation: true,
        ..Default::default()
    };
    let config =
        ContractClassManagerConfig { cairo_native_run_config: native_config, ..Default::default() };
    let manager = ContractClassManager::start(config);
    let (request, native) = create_test_request_with_native();
    let class_hash = request.0;
    manager.send_compilation_request(request);
    assert_eq!(
        manager.get_native(&class_hash),
        Some(CachedCairoNative::Compiled(native)),
        "Cached Native class should match the expected result"
    );
}

// TODO (AvivG): is this test redundant?
#[test]
fn test_send_compilation_request_channel_full() {
    let native_config = CairoNativeRunConfig {
        run_cairo_native: true,
        wait_on_native_compilation: true,
        ..Default::default()
    };
    let config =
        ContractClassManagerConfig { cairo_native_run_config: native_config, ..Default::default() };
    let manager = ContractClassManager::start(config);
    let request = create_test_request();
    let second_request = create_test_request();

    // Fill the channel (it can only hold 1 message)
    manager.send_compilation_request(request);
    // Should log an error without panicking
    manager.send_compilation_request(second_request);
}

#[test]
#[should_panic]
fn test_send_compilation_request_run_cairo_native_false() {
    let native_config = CairoNativeRunConfig {
        run_cairo_native: false,
        wait_on_native_compilation: true,
        ..Default::default()
    };
    let config =
        ContractClassManagerConfig { cairo_native_run_config: native_config, ..Default::default() };
    let manager = ContractClassManager::start(config);
    let request = create_test_request();
    manager.send_compilation_request(request);
    // TODO (AvivG): add massage: Expected panic when sending request with run_cairo_native false?
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
    let config =
        ContractClassManagerConfig { cairo_native_run_config: native_config, ..Default::default() };
    let manager = ContractClassManager::start(config);
    let (request, _native) = request_w_native;
    let compiler_config = SierraCompilationConfig::default();
    let compiler = Arc::new(CommandLineCompiler::new(compiler_config));
    process_compilation_request(manager.contract_caches.clone(), compiler.clone(), request.clone());

    let cached_native = manager.get_native(&request.0);
    assert_eq!(
        cached_native,
        Some(expected_cached_native),
        "Cached Native class should match the expected."
    );
}

#[rstest]
fn test_run_compilation_worker() {
    let native_config = CairoNativeRunConfig { run_cairo_native: true, ..Default::default() };
    let config =
        ContractClassManagerConfig { cairo_native_run_config: native_config, ..Default::default() };
    let contract_caches = ContractCaches::new(config.contract_cache_size);
    let (sender, receiver) = sync_channel(native_config.channel_size);
    let compiler_config = SierraCompilationConfig::default();
    let compiler = Arc::new(CommandLineCompiler::new(compiler_config));
    let (request, native) = create_test_request_with_native();
    sender.try_send(request.clone()).unwrap();
    drop(sender);
    let manager = ContractClassManager {
        cairo_native_run_config: native_config,
        contract_caches: contract_caches.clone(),
        sender: None,
        compiler: None,
    };

    run_compilation_worker(contract_caches.clone(), receiver, compiler);

    let cached_native = manager.get_native(&request.0);
    assert_eq!(
        cached_native,
        Some(CachedCairoNative::Compiled(native)),
        "Cached Native class should match the expected."
    );
}

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

<<<<<<< HEAD
fn create_test_request_with_native() -> (CompilationRequest, NativeCompiledClassV1) {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Native));
    let request = create_test_request_from_contract(test_contract);
    let native = match test_contract.get_runnable_class() {
        RunnableCompiledClass::V1Native(native) => native,
        _ => panic!("Expected NativeCompiledClassV1"),
    };
=======
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
>>>>>>> 56c5ea558 (chore(blockifier): create unit tests for contract_class_manager)

    (request, native)
}

<<<<<<< HEAD
#[test]
fn test_sender_with_native_compilation_disabled() {
    let config = ContractClassManagerConfig { run_cairo_native: false, ..Default::default() };
    let manager = ContractClassManager::start(config);
    assert!(manager.sender.is_none(), "Sender should be None when native compilation is disabled");
}

#[test]
fn test_sender_with_native_compilation_enabled() {
    let config = ContractClassManagerConfig { run_cairo_native: true, ..Default::default() };
    let manager = ContractClassManager::start(config);
    assert!(manager.sender.is_some());

    assert!(
        manager.sender.as_ref().unwrap().try_send(create_test_request()).is_ok(),
        "Sender should be able to send a request successfully"
    );
}

#[test]
fn test_send_request_channel_disconnected() {
    let config = ContractClassManagerConfig { run_cairo_native: true, ..Default::default() };
    let contract_caches = ContractCaches::new(config.contract_cache_size);
    let manager = ContractClassManager { config, contract_caches, sender: None, compiler: None };

    let request = create_test_request();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        manager.send_compilation_request(request);
    }));

    assert!(result.is_err(), "Expected panic when sending request with disconnected channel");
}

#[test]
fn test_send_compilation_request_channel_full() {
    let config = ContractClassManagerConfig {
        run_cairo_native: true,
        channel_size: 1,
        ..Default::default()
    };
    let manager = ContractClassManager::start(config);
    let request = create_test_request();
    let second_request = create_test_request();

    // Fill the channel (it can only hold 1 message)
    manager.send_compilation_request(request);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        manager.send_compilation_request(second_request);
    }));

    assert!(result.is_ok(), "Should not panic when channel is full.");
}

#[test]
fn test_run_compilation_worker_success() {
    let manager = create_test_contract_class_manager(TEST_CHANNEL_SIZE);
    let (request, native) = create_test_request_with_native();

    manager.sender.as_ref().unwrap().send(request.clone()).unwrap();

    // Wait for the worker to process the request
    // Question (AvivG): better to have a loop and try to get native every X mil sec?
    std::thread::sleep(std::time::Duration::from_millis(50000));

    let cached_native = manager.get_native(&request.0);

    assert!(cached_native.is_some(), "Native compiled class should exist in the cache");

    match cached_native.unwrap() {
        CachedCairoNative::Compiled(cached_class) => {
            assert_eq!(cached_class, native, "Cached class should match the expected native class");
        }
        CachedCairoNative::CompilationFailed => {
            panic!("Expected CachedCairoNative::Compiled variant")
        }
    };
}

#[test]
fn test_run_compilation_worker_failure() {
    let manager = create_test_contract_class_manager(TEST_CHANNEL_SIZE);

    let request = create_faulty_test_request();

    manager.sender.as_ref().unwrap().send(request.clone()).unwrap();

    // Wait for the worker to process the request
    // Question (AvivG): better to have a loop and try to get native every X mil sec?
    std::thread::sleep(std::time::Duration::from_millis(5000));

    // Check if the compilation-failed variant was added to the cache
    let cached_native = manager.get_native(&request.0);
    assert_eq!(
        cached_native,
        Some(CachedCairoNative::CompilationFailed),
        "Native compiled class should indicate compilation failure"
    );
}

#[test]
fn test_channel_receiver_down_when_sender_dropped() {
    // TODO (AvivG).
}

// TODO (AvivG):test compilation logs.

// #[test]
// fn test_get_casm() {
//     let config = ContractClassManagerConfig {
//         run_cairo_native: false,
//         ..Default::default()
//     };
//     let manager = ContractClassManager::start(config);
//     let class_hash = ClassHash::default();
//     assert!(manager.get_casm(&class_hash).is_none());
// }

// #[test]
// fn test_set_and_get_casm() {
//     let config = ContractClassManagerConfig {
//         run_cairo_native: false,
//         ..Default::default()
//     };
//     let manager = ContractClassManager::start(config);
//     let class_hash = ClassHash::default();
//     let compiled_class = CachedCasm::default();
//     manager.set_casm(class_hash, compiled_class.clone());
//     assert_eq!(manager.get_casm(&class_hash), Some(compiled_class));
// }

// #[test]
// fn test_clear_cache() {
//     let config = ContractClassManagerConfig {
//         run_cairo_native: false,
//         ..Default::default()
//     };
//     let mut manager = ContractClassManager::start(config);
//     let class_hash = ClassHash::default();
//     let compiled_class = CachedCasm::default();
//     manager.set_casm(class_hash, compiled_class);
//     manager.clear();
//     assert!(manager.get_casm(&class_hash).is_none());
=======
// TODO (AvivG): finish this test?
// #[test]
// fn test_compilation_request_not_sent_if_already_in_cache() {
//     let native_config = CairoNativeRunConfig {
//         run_cairo_native: true,
//         wait_on_native_compilation: true,
//         ..Default::default()
//     };
//     let config =
//      ContractClassManagerConfig { cairo_native_run_config: native_config, ..Default::default() };
//     let manager = ContractClassManager::start(config);
//     let (request1, native1) = create_test_request_with_native();
//     let (request2, native2) = create_test_request_with_native();
//     let class_hash = request1.0;
//     let compiler_config = SierraCompilationConfig::default();
//     let compiler = Arc::new(CommandLineCompiler::new(compiler_config));
//     // Send the first request
//     process_compilation_request(manager.contract_caches.clone(), compiler.clone(), request1);
//
//     // TODO (AvivG): track logs
//     // Send the first request again, sould not compile again.
//     process_compilation_request(manager.contract_caches.clone(), compiler.clone(), request2);

//     let expected_log = format!(
//         "Contract class with hash {} is already compiled to native. Skipping compilation.",
//         class_hash
//     );
//     // TODO (AvivG): fix assert
//     assert!(logs.iter().any(|log| log.contains(&expected_log)), "Expected log message not
//      found."); (?)
//     assert!(logger.contains(&expected_log), "Expected log message not found."); (?)

>>>>>>> 56c5ea558 (chore(blockifier): create unit tests for contract_class_manager)
// }
