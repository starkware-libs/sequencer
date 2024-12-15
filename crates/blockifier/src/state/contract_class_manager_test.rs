use std::sync::mpsc::sync_channel;
use std::sync::Arc;

use rstest::rstest;
use starknet_api::contract_class::ContractClass;
use starknet_sierra_multicompile::command_line_compiler::CommandLineCompiler;
use starknet_sierra_multicompile::config::SierraCompilationConfig;

use crate::blockifier::config::CairoNativeRunConfig;
use crate::execution::contract_class::RunnableCompiledClass;
use crate::execution::native::contract_class::NativeCompiledClassV1;
use crate::state::contract_class_manager::{
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
// use crate::test_utils::struct_impls::ContractClassManager;
type TestRequestWithNative = (CompilationRequest, NativeCompiledClassV1);

const TEST_CHANNEL_SIZE: usize = 10;

fn get_native(test_contract: FeatureContract) -> NativeCompiledClassV1 {
    match test_contract.get_runnable_class() {
        RunnableCompiledClass::V1Native(native) => native,
        _ => panic!("Expected NativeCompiledClassV1"),
    }
}

fn create_test_request_from_contract(test_contract: FeatureContract) -> CompilationRequest {
    let class_hash = test_contract.get_class_hash();
    let sierra = Arc::new(test_contract.get_sierra());

    // Generate Casm, no need to match the native.
    let contract_class: ContractClass =
        FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm)).get_class();
    let casm = match contract_class {
        ContractClass::V1(compiled_class) => compiled_class.try_into().unwrap(),
        _ => panic!("Expected CompiledClassV1"),
    };

    // 2nd option
    // use crate::test_utils::struct_impls::LoadContractFromFile;
    // use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;

    // let contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));
    // let casm = ( CasmContractClass::from_file(&contract.get_compiled_path()),
    //     contract.get_sierra_version()).try_into().unwrap();

    (class_hash, sierra, casm)
}

fn create_test_request() -> CompilationRequest {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Native));
    create_test_request_from_contract(test_contract)
}

fn create_test_request_with_native() -> TestRequestWithNative {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Native));
    let request = create_test_request_from_contract(test_contract);
    let native = get_native(test_contract);

    (request, native)
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
