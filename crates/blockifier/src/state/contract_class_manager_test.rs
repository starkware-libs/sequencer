#[cfg(feature = "cairo_native")]
use std::sync::mpsc::sync_channel;
#[cfg(feature = "cairo_native")]
use std::sync::Arc;

#[cfg(feature = "cairo_native")]
use rstest::rstest;
#[cfg(feature = "cairo_native")]
use starknet_sierra_compile::command_line_compiler::CommandLineCompiler;
#[cfg(feature = "cairo_native")]
use starknet_sierra_compile::config::SierraToCasmCompilationConfig;

#[cfg(feature = "cairo_native")]
use crate::blockifier::config::ContractClassManagerConfig;
// use crate::concurrency::test_utils::class_hash;
#[cfg(feature = "cairo_native")]
use crate::execution::contract_class::RunnableCompiledClass;
#[cfg(all(test, feature = "cairo_native"))]
use crate::execution::native::contract_class::NativeCompiledClassV1;
#[cfg(feature = "cairo_native")]
use crate::state::contract_class_manager::process_compilation_request;
#[cfg(all(test, feature = "cairo_native"))]
use crate::state::contract_class_manager::CompilationRequest;
#[cfg(feature = "cairo_native")]
use crate::state::contract_class_manager::ContractClassManager;
#[cfg(feature = "cairo_native")]
use crate::state::global_cache::{CachedCairoNative, ContractCaches};
#[cfg(feature = "cairo_native")]
use crate::test_utils::contracts::FeatureContract;
#[cfg(feature = "cairo_native")]
use crate::test_utils::{CairoVersion, RunnableCairo1};

#[cfg(feature = "cairo_native")]
type TestRequestWithNative = (CompilationRequest, NativeCompiledClassV1);
#[cfg(feature = "cairo_native")]
const TEST_CHANNEL_SIZE: usize = 10;

#[cfg(feature = "cairo_native")]
#[rstest]
fn test_start(
    #[values(true, false)] run_cairo_native: bool,
    #[values(true, false)] wait_on_native_compilation: bool,
) {
    let config = ContractClassManagerConfig {
        run_cairo_native,
        wait_on_native_compilation,
        ..Default::default()
    };
    let manager = ContractClassManager::start(config.clone());
    // TODO(AvivG): any constraints on initial caches? should start empty?
    // TODO(AvivG): any checks for not cairo_native? #[cfg(not(feature = "cairo_native"))]

    assert_eq!(manager.config, config);
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
}

#[cfg(feature = "cairo_native")]
#[test]
#[should_panic]
fn test_send_compilation_request_channel_disconnected() {
    let config = ContractClassManagerConfig { run_cairo_native: true, ..Default::default() };
    let contract_caches = ContractCaches::new(config.contract_cache_size);
    let (sender, receiver) = sync_channel(config.channel_size);
    drop(receiver);
    let manager =
        ContractClassManager { config, contract_caches, sender: Some(sender), compiler: None };

    let request = create_test_request();
    // TODO(AvivG): add massage: Expected panic when sending request with disconnected channel
    manager.send_compilation_request(request);
}

#[cfg(feature = "cairo_native")]
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
    // Should log an error without panicking
    manager.send_compilation_request(second_request);
    // TODO(AvivG): how to add massage? : "Should not panic when channel is full.";
}

#[cfg(feature = "cairo_native")]
#[test]
fn test_send_compilation_request_wait_on_native() {
    let config = ContractClassManagerConfig {
        run_cairo_native: true,
        wait_on_native_compilation: true,
        ..Default::default()
    };
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

#[cfg(feature = "cairo_native")]
#[test]
#[should_panic]
fn test_send_compilation_request_run_cairo_native_false() {
    let config = ContractClassManagerConfig {
        run_cairo_native: false,
        wait_on_native_compilation: true,
        ..Default::default()
    };
    let manager = ContractClassManager::start(config);
    let request = create_test_request();
    manager.send_compilation_request(request);
    // TODO (AvivG): add massage: Expected panic when sending request with run_cairo_native false
}

#[cfg(feature = "cairo_native")]
#[rstest]
#[case::success(create_test_request_with_native(), CachedCairoNative::Compiled(create_test_request_with_native().1))]
#[case::failure(create_faulty_test_request(), CachedCairoNative::CompilationFailed)]
fn test_process_compilation_request(
    #[case] request_w_native: TestRequestWithNative,
    #[case] expected_cache: CachedCairoNative,
) {
    // let manager = create_test_contract_class_manager(TEST_CHANNEL_SIZE);
    let config = ContractClassManagerConfig {
        run_cairo_native: true,
        channel_size: TEST_CHANNEL_SIZE,
        wait_on_native_compilation: true,
        ..Default::default()
    };
    let manager = ContractClassManager::start(config);
    let (request, _native) = request_w_native;
    let compiler_config = SierraToCasmCompilationConfig::default();
    let compiler = Arc::new(CommandLineCompiler::new(compiler_config));
    process_compilation_request(manager.contract_caches.clone(), compiler.clone(), request.clone());

    let cached_native = manager.get_native(&request.0);
    assert_eq!(
        cached_native,
        Some(expected_cache),
        "Cached Native class should match the expected result"
    );
}

#[cfg(feature = "cairo_native")]
#[rstest]
#[case(false, "Sender should be None when native compilation is disabled")]
#[case(true, "Sender should be Some when native compilation is enabled")]
fn test_sender_with_native_compilation(#[case] run_cairo_native: bool, #[case] message: &str) {
    let config = ContractClassManagerConfig { run_cairo_native, ..Default::default() };
    let manager = ContractClassManager::start(config);

    if run_cairo_native {
        assert!(manager.sender.is_some(), "{}", message);
        assert!(
            manager.sender.as_ref().unwrap().try_send(create_test_request()).is_ok(),
            "Sender should be able to send a request successfully"
        );
    } else {
        assert!(manager.sender.is_none(), "{}", message);
    }
}

#[cfg(all(test, feature = "cairo_native"))]
fn create_faulty_test_request() -> TestRequestWithNative {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Native));
    create_faulty_request(test_contract)
}

#[cfg(all(test, feature = "cairo_native"))]
fn create_faulty_request(test_contract: FeatureContract) -> TestRequestWithNative {
    let class_hash = test_contract.get_class_hash();
    let casm = test_contract.get_casm();
    let mut sierra = test_contract.get_sierra();
    // Truncate the sierra program to trigger an error.
    sierra.sierra_program = sierra.sierra_program[..100].to_vec();

    let request = (class_hash, Arc::new(sierra), casm);

    (request, get_native(test_contract))
}

#[cfg(feature = "cairo_native")]
fn create_test_request_from_contract(test_contract: FeatureContract) -> CompilationRequest {
    let class_hash = test_contract.get_class_hash();
    let sierra = Arc::new(test_contract.get_sierra());
    let casm = test_contract.get_casm();

    (class_hash, sierra, casm)
}

#[cfg(all(test, feature = "cairo_native"))]
fn create_test_request() -> CompilationRequest {
    // Question (AvivG): are we interested in testing other contracts?
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Native));
    create_test_request_from_contract(test_contract)
}

#[cfg(all(test, feature = "cairo_native"))]
fn get_native(test_contract: FeatureContract) -> NativeCompiledClassV1 {
    match test_contract.get_runnable_class() {
        RunnableCompiledClass::V1Native(native) => native,
        _ => panic!("Expected NativeCompiledClassV1"),
    }
}

#[cfg(all(test, feature = "cairo_native"))]
fn create_test_request_with_native() -> TestRequestWithNative {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Native));
    let request = create_test_request_from_contract(test_contract);
    let native = get_native(test_contract);

    (request, native)
}

// TODO (AvivG): Add tests for:
//  getters?
//  setters?
//  clear
//  process_compilation_request
//  run_compilation_worker

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
// }
