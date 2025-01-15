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
use crate::test_utils::{initialize_logger, CairoVersion, RunnableCairo1, TEST_LOGGER};

type TestRequestWithNative = (CompilationRequest, NativeCompiledClassV1);

const TEST_CHANNEL_SIZE: usize = 10;
const TEST_PROCESS_COMPILATION_REQUEST_FAILURE_LOG: &str =
    "Error compiling contract class: Starknet Sierra compilation error: Exit status: exit status: \
     1\n Stderr: Error extracting Sierra program from contract class: Invalid input for \
     deserialization.\n";

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

fn create_faulty_request() -> TestRequestWithNative {
    let ((class_hash, sierra, casm), native) = create_test_request_with_native();
    let mut sierra = sierra.as_ref().clone();

    // Truncate the sierra program to trigger an error.
    sierra.sierra_program = sierra.sierra_program[..100].to_vec();

    let request = (class_hash, Arc::new(sierra), casm);

    (request, native)
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

#[rstest]
#[case::run_native_while_waiting(true, true)]
#[case::run_native_without_waiting(true, false)]
#[should_panic(expected = "Native compilation is disabled.")]
#[case::run_without_native(false, false)]
fn test_send_compilation_request_positive_flow(
    #[case] run_cairo_native: bool,
    #[case] wait_on_native_compilation: bool,
) {
    initialize_logger();
    let native_config =
        CairoNativeRunConfig { run_cairo_native, wait_on_native_compilation, ..Default::default() };
    let config =
        ContractClassManagerConfig { cairo_native_run_config: native_config, ..Default::default() };
    let manager = ContractClassManager::start(config);
    let (request, native) = create_test_request_with_native();
    let class_hash = request.0;
    manager.send_compilation_request(request);
    if wait_on_native_compilation {
        if let CachedCairoNative::Compiled(compiled_native) = manager.get_native(&class_hash).unwrap() {
            assert_eq!(compiled_native, native);
        } else {
            println!("{:?}", TEST_LOGGER.get_logs());
            panic!("Cached Native class should match the expected result");
        }
    } else {
        let expected_log =
            format!("Compilation request with class hash: {} was sent successfully.", class_hash);
        assert!(TEST_LOGGER.contains(expected_log.as_str()));
    }
    TEST_LOGGER.clear_logs();
}

#[test]
fn test_send_compilation_request_channel_full() {
    initialize_logger();
    let native_config = CairoNativeRunConfig {
        run_cairo_native: true,
        wait_on_native_compilation: false,
        channel_size: 0,
    };
    let manager = create_test_manager(native_config);
    let request = create_test_request();
    let class_hash = request.0;

    assert!(manager.sender.is_some(), "Sender should be Some");

    // Fill the channel (it can only hold 1 message)
    manager.send_compilation_request(request.clone());
    // Should log an error without panicking
    manager.send_compilation_request(request);

    let expected_log = format!(
        "Compilation request channel is full (size: {}). Compilation request for class hash {} \
         was not sent.",
        manager.cairo_native_run_config.channel_size, class_hash
    );
    assert!(TEST_LOGGER.contains(expected_log.as_str()));
    TEST_LOGGER.clear_logs();
}

#[rstest]
#[case::success(create_test_request_with_native(), CachedCairoNative::Compiled(create_test_request_with_native().1), None)]
#[case::failure(
    create_faulty_request(),
    CachedCairoNative::CompilationFailed,
    Some(TEST_PROCESS_COMPILATION_REQUEST_FAILURE_LOG)
)]
fn test_process_compilation_request(
    #[case] request_w_native: TestRequestWithNative,
    #[case] expected_cached_native: CachedCairoNative,
    #[case] expected_log: Option<&str>,
) {
    initialize_logger();
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

    if let Some(expected_log) = expected_log {
        assert!(TEST_LOGGER.contains(expected_log));
    }
    match expected_cached_native {
        CachedCairoNative::Compiled(compiled_native) => {
            let cached_native = manager.get_native(&request.0).unwrap();
            if let CachedCairoNative::Compiled(cached_native) = cached_native {
                assert_eq!(cached_native, compiled_native);
            } else {
                println!("{:?}", TEST_LOGGER.get_logs());
                panic!("Cached Native class should match the expected result");
            }
        }
        CachedCairoNative::CompilationFailed => {
            assert!(matches!(
                manager.get_native(&request.0).unwrap(),
                CachedCairoNative::CompilationFailed
            ));
        }

    }
    TEST_LOGGER.clear_logs();

}

#[rstest]
fn test_run_compilation_worker() {
    initialize_logger();
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
    let cache_native = contract_caches.get_native(&request.0).unwrap();
    if let CachedCairoNative::Compiled(compiled_native) = cache_native {
        assert_eq!(compiled_native, expected_native);
    } else {
        println!("{:?}", TEST_LOGGER.get_logs());
        panic!("Cached Native class should match the expected result");
    }
    TEST_LOGGER.clear_logs();

}

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
    let (request1, _) = create_test_request_with_native();
    let (request2, _) = create_test_request_with_native();
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
    TEST_LOGGER.clear_logs();
}
