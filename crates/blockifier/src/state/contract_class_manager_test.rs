use std::sync::mpsc::sync_channel;
use std::sync::{Arc, LazyLock, Once};

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
use crate::test_utils::{CairoVersion, RunnableCairo1, TestLogger};

type TestRequestWithNative = (CompilationRequest, NativeCompiledClassV1);

const TEST_CHANNEL_SIZE: usize = 10;

static TEST_LOGGER: LazyLock<TestLogger> = LazyLock::new(TestLogger::new);

static INIT_LOGGER: Once = Once::new();

fn initialize_logger() {
    INIT_LOGGER.call_once(|| {
        log::set_logger(&*TEST_LOGGER).unwrap();
        log::set_max_level(log::LevelFilter::Debug);
    });
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
#[should_panic(expected = "Compilation request channel is closed.")]
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
        manager.get_native(&class_hash).unwrap(),
        CachedCairoNative::Compiled(native),
        "Cached Native class should match the expected result"
    );
}

#[test]
fn test_send_compilation_request_channel_full() {
    initialize_logger();
    let native_config = CairoNativeRunConfig {
        run_cairo_native: true,
        wait_on_native_compilation: false,
        channel_size: 1,
    };
    let config =
        ContractClassManagerConfig { cairo_native_run_config: native_config, ..Default::default() };
    let manager = ContractClassManager::start(config);
    let request = create_test_request();
    let second_request = create_test_request();
    let class_hash = second_request.0;

    // Fill the channel (it can only hold 1 message)
    manager.send_compilation_request(request);
    // Should log an error without panicking
    manager.send_compilation_request(second_request);

    let expected_log = format!(
        "Compilation request channel is full (size: {}). Compilation request for class hash {} \
         was not sent.",
        manager.cairo_native_run_config.channel_size, class_hash
    );
    assert!(TEST_LOGGER.contains(expected_log.as_str()));
}

#[test]
#[should_panic(expected = "Native compilation is disabled.")]
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
        cached_native.unwrap(),
        CachedCairoNative::Compiled(native),
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

// TODO (AvivG): finish this test?
#[test]
fn test_compilation_request_not_sent_if_already_in_cache() {
    initialize_logger();
    let native_config = CairoNativeRunConfig {
        run_cairo_native: true,
        wait_on_native_compilation: true,
        ..Default::default()
    };
    let config =
        ContractClassManagerConfig { cairo_native_run_config: native_config, ..Default::default() };
    let manager = ContractClassManager::start(config);
    let (request1, _native1) = create_test_request_with_native();
    let (request2, _native2) = create_test_request_with_native();
    assert_eq!(
        request1.0, request2.0,
        "Both requests should have the same class hash in this test."
    );
    let class_hash = request1.0;
    let compiler_config = SierraCompilationConfig::default();
    let compiler = Arc::new(CommandLineCompiler::new(compiler_config));
    // Send the first request
    process_compilation_request(manager.contract_caches.clone(), compiler.clone(), request1);
    assert!(manager.get_native(&class_hash).is_some(), "Native class should be in the cache.");

    // Send the first request again, should not compile again.
    process_compilation_request(manager.contract_caches.clone(), compiler.clone(), request2);
    let expected_log = format!(
        "Contract class with hash {} is already compiled to native. Skipping compilation.",
        class_hash
    );
    assert!(TEST_LOGGER.contains(expected_log.as_str()));
}
