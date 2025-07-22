use std::sync::mpsc::{sync_channel, TrySendError};
use std::sync::Arc;
use std::thread::sleep;

use apollo_compilation_utils::errors::CompilationUtilError;
use apollo_compile_to_native_types::DEFAULT_MAX_CPU_TIME;
use assert_matches::assert_matches;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::contracts::FeatureContract;
use rstest::rstest;
use starknet_api::core::ClassHash;

use crate::blockifier::config::{CairoNativeRunConfig, NativeClassesWhitelist};
use crate::execution::contract_class::{CompiledClassV1, RunnableCompiledClass};
use crate::state::global_cache::{
    CachedCairoNative,
    CompiledClasses,
    RawClassCache,
    GLOBAL_CONTRACT_CACHE_SIZE_FOR_TEST,
};
use crate::state::native_class_manager::{
    process_compilation_request,
    CompilationRequest,
    ContractClassManagerError,
    NativeClassManager,
};
use crate::test_utils::contracts::FeatureContractTrait;

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

fn get_test_contract_class_hash() -> ClassHash {
    FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm)).get_class_hash()
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
    let manager = NativeClassManager::create_for_testing(native_config.clone());

    assert_eq!(manager.cairo_native_run_config.clone(), native_config);
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
#[case::run_without_native(false, true)]
#[case::run_without_native(false, false)]
fn test_set_and_compile(
    #[case] run_cairo_native: bool,
    #[case] wait_on_native_compilation: bool,
    #[values(true, false)] should_pass: bool,
) {
    let native_config =
        CairoNativeRunConfig { run_cairo_native, wait_on_native_compilation, ..Default::default() };
    let manager = NativeClassManager::create_for_testing(native_config);
    let request = if should_pass { create_test_request() } else { create_faulty_request() };
    let class_hash = request.0;
    let (_class_hash, sierra, casm) = request.clone();
    let compiled_class = CompiledClasses::V1(casm, sierra);

    manager.set_and_compile(class_hash, compiled_class);
    if !run_cairo_native {
        assert_matches!(manager.cache.get(&class_hash).unwrap(), CompiledClasses::V1(_, _));
        return;
    }

    if !wait_on_native_compilation {
        assert_matches!(manager.cache.get(&class_hash).unwrap(), CompiledClasses::V1(_, _));
        let seconds_to_sleep = 2;
        let max_n_retries = DEFAULT_MAX_CPU_TIME / seconds_to_sleep + 1;
        for _ in 0..max_n_retries {
            sleep(std::time::Duration::from_secs(seconds_to_sleep));
            if matches!(manager.cache.get(&class_hash), Some(CompiledClasses::V1Native(_))) {
                break;
            }
        }
    }

    match manager.cache.get(&class_hash).unwrap() {
        CompiledClasses::V1Native(CachedCairoNative::Compiled(_)) => {
            assert!(should_pass, "Compilation should have passed.");
        }
        CompiledClasses::V1Native(CachedCairoNative::CompilationFailed(_)) => {
            assert!(!should_pass, "Compilation should have failed.");
        }
        CompiledClasses::V1(_, _) | CompiledClasses::V0(_) => {
            panic!("Unexpected compiled class.");
        }
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
        ..CairoNativeRunConfig::default()
    };
    let (sender, receiver) = sync_channel(native_config.channel_size);
    let manager = NativeClassManager {
        cairo_native_run_config: native_config,
        cache: RawClassCache::new(GLOBAL_CONTRACT_CACHE_SIZE_FOR_TEST),
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
        ..CairoNativeRunConfig::default()
    };
    let manager = NativeClassManager::create_for_testing(native_config);
    let request = create_test_request();
    assert!(manager.sender.is_some(), "Sender should be Some");

    // Fill the channel (it can only hold 1 message).
    manager.send_compilation_request(request.clone()).unwrap();
    let result = manager.send_compilation_request(request.clone());
    assert_eq!(
        result.unwrap_err(),
        ContractClassManagerError::TrySendError(TrySendError::Full(request.0))
    );
}

#[rstest]
#[case::success(create_test_request(), true, false)]
#[case::failure(create_faulty_request(), false, false)]
#[should_panic(expected = "Compilation failed")]
#[case::panics_on_failure(create_faulty_request(), false, true)]
fn test_process_compilation_request(
    #[case] request: CompilationRequest,
    #[case] should_pass: bool,
    #[case] panic_on_compilation_failure: bool,
) {
    let manager = NativeClassManager::create_for_testing(CairoNativeRunConfig {
        wait_on_native_compilation: true,
        run_cairo_native: true,
        channel_size: TEST_CHANNEL_SIZE,
        panic_on_compilation_failure,
        ..CairoNativeRunConfig::default()
    });
    let res = process_compilation_request(
        manager.clone().cache,
        manager.clone().compiler.unwrap(),
        request.clone(),
        manager.cairo_native_run_config.panic_on_compilation_failure,
    );

    if should_pass {
        assert!(
            res.is_ok(),
            "Compilation request failed with the following error: {}.",
            res.unwrap_err()
        );
        assert_matches!(
            manager.cache.get(&request.0).unwrap(),
            CompiledClasses::V1Native(CachedCairoNative::Compiled(_))
        );
    } else {
        assert_matches!(res.unwrap_err(), CompilationUtilError::CompilationError(_));
        assert_matches!(
            manager.cache.get(&request.0).unwrap(),
            CompiledClasses::V1Native(CachedCairoNative::CompilationFailed(_))
        );
    }
}

#[rstest]
#[case::all_classes(NativeClassesWhitelist::All, true)]
#[case::only_selected_class_hash(NativeClassesWhitelist::Limited(vec![get_test_contract_class_hash()]), true)]
#[case::no_allowed_classes(NativeClassesWhitelist::Limited(vec![]), false)]
// Test the config that allows us to run only limited selection of class hashes in native.
fn test_native_classes_whitelist(
    #[case] whitelist: NativeClassesWhitelist,
    #[case] allow_run_native: bool,
) {
    let native_config = CairoNativeRunConfig {
        run_cairo_native: true,
        wait_on_native_compilation: true,
        panic_on_compilation_failure: true,
        channel_size: TEST_CHANNEL_SIZE,
        native_classes_whitelist: whitelist,
    };
    let manager = NativeClassManager::create_for_testing(native_config);

    let (class_hash, sierra, casm) = create_test_request();

    manager.set_and_compile(class_hash, CompiledClasses::V1(casm, sierra));

    match allow_run_native {
        true => assert_matches!(
            manager.get_runnable(&class_hash),
            Some(RunnableCompiledClass::V1Native(_))
        ),
        false => {
            assert_matches!(
                manager.get_runnable(&class_hash).unwrap(),
                RunnableCompiledClass::V1(_)
            )
        }
    }
}
