use assert_matches::assert_matches;
use blockifier::execution::contract_class::ContractClass;
use cairo_lang_starknet_classes::allowed_libfuncs::AllowedLibfuncsError;
use mempool_test_utils::starknet_api_test_utils::declare_tx;
use rstest::{fixture, rstest};
use starknet_api::core::CompiledClassHash;
use starknet_api::rpc_transaction::{RpcDeclareTransaction, RpcTransaction};
use starknet_sierra_compile::errors::CompilationUtilError;
use tracing_test::traced_test;

use crate::compilation::GatewayCompiler;
use crate::errors::GatewaySpecError;

#[fixture]
fn gateway_compiler() -> GatewayCompiler {
    GatewayCompiler { config: Default::default() }
}

// TODO(Arni): Redesign this test once the compiler is passed with dependancy injection.
#[traced_test]
#[rstest]
fn test_compile_contract_class_compiled_class_hash_mismatch(gateway_compiler: GatewayCompiler) {
    let mut tx = assert_matches!(
        declare_tx(),
        RpcTransaction::Declare(RpcDeclareTransaction::V3(tx)) => tx
    );
    let expected_hash = tx.compiled_class_hash;
    let wrong_supplied_hash = CompiledClassHash::default();
    tx.compiled_class_hash = wrong_supplied_hash;
    let declare_tx = RpcDeclareTransaction::V3(tx);

    let err = gateway_compiler.process_declare_tx(&declare_tx).unwrap_err();
    assert_eq!(err, GatewaySpecError::CompiledClassHashMismatch);
    assert!(logs_contain(
        format!(
            "Compiled class hash mismatch. Supplied: {:?}, Hash result: {:?}",
            wrong_supplied_hash, expected_hash
        )
        .as_str()
    ));
}

#[traced_test]
#[rstest]
fn test_compile_contract_class_bad_sierra(gateway_compiler: GatewayCompiler) {
    let mut tx = assert_matches!(
        declare_tx(),
        RpcTransaction::Declare(RpcDeclareTransaction::V3(tx)) => tx
    );
    // Truncate the sierra program to trigger an error.
    tx.contract_class.sierra_program = tx.contract_class.sierra_program[..100].to_vec();
    let declare_tx = RpcDeclareTransaction::V3(tx);

    let err = gateway_compiler.process_declare_tx(&declare_tx).unwrap_err();
    assert_eq!(err, GatewaySpecError::CompilationFailed);

    let expected_compilation_error =
        CompilationUtilError::AllowedLibfuncsError(AllowedLibfuncsError::SierraProgramError);
    assert!(logs_contain(format!("Compilation failed: {:?}", expected_compilation_error).as_str()));
}

#[rstest]
fn test_process_declare_tx_success(gateway_compiler: GatewayCompiler) {
    let declare_tx = assert_matches!(
        declare_tx(),
        RpcTransaction::Declare(declare_tx) => declare_tx
    );
    let RpcDeclareTransaction::V3(declare_tx_v3) = &declare_tx;
    let contract_class = &declare_tx_v3.contract_class;

    let class_info = gateway_compiler.process_declare_tx(&declare_tx).unwrap();
    assert_matches!(class_info.contract_class(), ContractClass::V1(_));
    assert_eq!(class_info.sierra_program_length(), contract_class.sierra_program.len());
    assert_eq!(class_info.abi_length(), contract_class.abi.len());
}
