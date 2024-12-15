// use std::sync::mpsc::sync_channel;
use std::sync::Arc;

use crate::blockifier::config::ContractClassManagerConfig;
use crate::execution::contract_class::RunnableCompiledClass;
use crate::execution::native::contract_class::NativeCompiledClassV1;
use crate::state::contract_class_manager::{CompilationRequest, ContractClassManager};
use crate::state::global_cache::{CachedCairoNative, ContractCaches};
use crate::test_utils::contracts::FeatureContract;
use crate::test_utils::{CairoVersion, RunnableCairo1};

const TEST_CHANNEL_SIZE: usize = 10;

fn create_faulty_test_request() -> CompilationRequest {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Native));

    let mut contract_class = test_contract.get_contract_class();
    // Truncate the sierra program to trigger an error.
    contract_class.sierra_program = contract_class.sierra_program[..100].to_vec();

    let sierra = contract_class.into();
    let class_hash = test_contract.get_class_hash();
    let casm = test_contract.get_casm();

    (class_hash, Arc::new(sierra), casm)
}

fn create_test_contract_class_manager(channel_size: usize) -> ContractClassManager {
    let config =
        ContractClassManagerConfig { run_cairo_native: true, channel_size, ..Default::default() };

    ContractClassManager::start(config)
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

fn create_test_request_with_native() -> (CompilationRequest, NativeCompiledClassV1) {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Native));
    let request = create_test_request_from_contract(test_contract);
    let native = match test_contract.get_runnable_class() {
        RunnableCompiledClass::V1Native(native) => native,
        _ => panic!("Expected NativeCompiledClassV1"),
    };

    (request, native)
}

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
// }
