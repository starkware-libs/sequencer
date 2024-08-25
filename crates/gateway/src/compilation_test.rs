use assert_matches::assert_matches;
use mempool_test_utils::starknet_api_test_utils::{
    declare_tx as rpc_declare_tx,
    COMPILED_CLASS_HASH,
};
use rstest::{fixture, rstest};
use starknet_api::rpc_transaction::{
    RpcDeclareTransaction,
    RpcDeclareTransactionV3,
    RpcTransaction,
};
use starknet_gateway_types::errors::GatewaySpecError;
use starknet_sierra_compile::config::SierraToCasmCompilationConfig;
use starknet_sierra_compile::errors::CompilationUtilError;
use tracing_test::traced_test;

use crate::compilation::GatewayCompiler;

#[fixture]
fn gateway_compiler() -> GatewayCompiler {
    GatewayCompiler::new_command_line_compiler(SierraToCasmCompilationConfig::default())
}

#[fixture]
fn declare_tx_v3() -> RpcDeclareTransactionV3 {
    assert_matches!(
        rpc_declare_tx(),
        RpcTransaction::Declare(RpcDeclareTransaction::V3(declare_tx)) => declare_tx
    )
}

// TODO(Arni): Redesign this test once the compiler is passed with dependancy injection.
#[traced_test]
#[rstest]
fn test_compile_contract_class_bytecode_size_validation(declare_tx_v3: RpcDeclareTransactionV3) {
    let gateway_compiler =
        GatewayCompiler::new_command_line_compiler(SierraToCasmCompilationConfig {
            max_bytecode_size: 1,
        });

    let result = gateway_compiler.process_declare_tx(&RpcDeclareTransaction::V3(declare_tx_v3));
    assert_matches!(result.unwrap_err(), GatewaySpecError::CompilationFailed);
    let expected_compilation_error = CompilationUtilError::CompilationError(
        "Error: Compilation failed.\n\nCaused by:\n    Code size limit exceeded.\n".to_owned(),
    );
    assert!(logs_contain(format!("Compilation failed: {:?}", expected_compilation_error).as_str()));
}

#[traced_test]
#[rstest]
fn test_compile_contract_class_bad_sierra(
    gateway_compiler: GatewayCompiler,
    mut declare_tx_v3: RpcDeclareTransactionV3,
) {
    // Truncate the sierra program to trigger an error.
    declare_tx_v3.contract_class.sierra_program =
        declare_tx_v3.contract_class.sierra_program[..100].to_vec();
    let declare_tx = RpcDeclareTransaction::V3(declare_tx_v3);

    let err = gateway_compiler.process_declare_tx(&declare_tx).unwrap_err();
    assert_eq!(err, GatewaySpecError::CompilationFailed);

    let expected_compilation_error =
        CompilationUtilError::CompilationError("Error: Invalid Sierra program.\n".to_owned());
    assert!(logs_contain(format!("Compilation failed: {:?}", expected_compilation_error).as_str()));
}

#[rstest]
fn test_process_declare_tx_success(
    gateway_compiler: GatewayCompiler,
    declare_tx_v3: RpcDeclareTransactionV3,
) {
    let contract_class = &declare_tx_v3.contract_class;
    let sierra_program_length = contract_class.sierra_program.len();
    let abi_length = contract_class.abi.len();
    let declare_tx = RpcDeclareTransaction::V3(declare_tx_v3);

    let class_info = gateway_compiler.process_declare_tx(&declare_tx).unwrap();
    let compiled_class_hash = class_info.contract_class.compiled_class_hash();
    assert_eq!(compiled_class_hash, *COMPILED_CLASS_HASH);
    assert_eq!(class_info.sierra_program_length, sierra_program_length);
    assert_eq!(class_info.abi_length, abi_length);
}
