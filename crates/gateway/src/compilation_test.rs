use assert_matches::assert_matches;
use blockifier::execution::contract_class::ContractClass;
use cairo_lang_starknet_classes::allowed_libfuncs::AllowedLibfuncsError;
use mempool_test_utils::starknet_api_test_utils::declare_tx;
use rstest::{fixture, rstest};
use starknet_api::core::CompiledClassHash;
use starknet_api::rpc_transaction::{RpcDeclareTransaction, RpcTransaction};
use starknet_sierra_compile::errors::CompilationUtilError;

use crate::compilation::GatewayCompiler;
use crate::errors::GatewayError;

#[fixture]
fn gateway_compiler() -> GatewayCompiler {
    GatewayCompiler { config: Default::default() }
}

#[rstest]
fn test_compile_contract_class_compiled_class_hash_missmatch(gateway_compiler: GatewayCompiler) {
    let mut tx = assert_matches!(
        declare_tx(),
        RpcTransaction::Declare(RpcDeclareTransaction::V3(tx)) => tx
    );
    let expected_hash_result = tx.compiled_class_hash;
    let supplied_hash = CompiledClassHash::default();

    tx.compiled_class_hash = supplied_hash;
    let declare_tx = RpcDeclareTransaction::V3(tx);

    let result = gateway_compiler.compile_contract_class(&declare_tx);
    assert_matches!(
        result.unwrap_err(),
        GatewayError::CompiledClassHashMismatch { supplied, hash_result }
        if supplied == supplied_hash && hash_result == expected_hash_result
    );
}

#[rstest]
fn test_compile_contract_class_bad_sierra(gateway_compiler: GatewayCompiler) {
    let mut tx = assert_matches!(
        declare_tx(),
        RpcTransaction::Declare(RpcDeclareTransaction::V3(tx)) => tx
    );
    // Truncate the sierra program to trigger an error.
    tx.contract_class.sierra_program = tx.contract_class.sierra_program[..100].to_vec();
    let declare_tx = RpcDeclareTransaction::V3(tx);

    let result = gateway_compiler.compile_contract_class(&declare_tx);
    assert_matches!(
        result.unwrap_err(),
        GatewayError::CompilationError(CompilationUtilError::AllowedLibfuncsError(
            AllowedLibfuncsError::SierraProgramError
        ))
    )
}

#[rstest]
fn test_compile_contract_class(gateway_compiler: GatewayCompiler) {
    let declare_tx = assert_matches!(
        declare_tx(),
        RpcTransaction::Declare(declare_tx) => declare_tx
    );
    let RpcDeclareTransaction::V3(declare_tx_v3) = &declare_tx;
    let contract_class = &declare_tx_v3.contract_class;

    let class_info = gateway_compiler.compile_contract_class(&declare_tx).unwrap();
    assert_matches!(class_info.contract_class(), ContractClass::V1(_));
    assert_eq!(class_info.sierra_program_length(), contract_class.sierra_program.len());
    assert_eq!(class_info.abi_length(), contract_class.abi.len());
}
