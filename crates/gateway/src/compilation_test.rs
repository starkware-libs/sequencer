use assert_matches::assert_matches;
use blockifier::execution::contract_class::ContractClass;
use cairo_lang_starknet_classes::allowed_libfuncs::AllowedLibfuncsError;
use starknet_api::core::CompiledClassHash;
use starknet_api::rpc_transaction::{RPCDeclareTransaction, RPCTransaction};
use starknet_sierra_compile::errors::CompilationUtilError;
use test_utils::starknet_api_test_utils::declare_tx;

use crate::compilation::compile_contract_class;
use crate::errors::GatewayError;

#[test]
fn test_compile_contract_class_compiled_class_hash_missmatch() {
    let mut tx = assert_matches!(
        declare_tx(),
        RPCTransaction::Declare(RPCDeclareTransaction::V3(tx)) => tx
    );
    let expected_hash_result = tx.compiled_class_hash;
    let supplied_hash = CompiledClassHash::default();

    tx.compiled_class_hash = supplied_hash;
    let declare_tx = RPCDeclareTransaction::V3(tx);

    let result = compile_contract_class(&declare_tx);
    assert_matches!(
        result.unwrap_err(),
        GatewayError::CompiledClassHashMismatch { supplied, hash_result }
        if supplied == supplied_hash && hash_result == expected_hash_result
    );
}

#[test]
fn test_compile_contract_class_bad_sierra() {
    let mut tx = assert_matches!(
        declare_tx(),
        RPCTransaction::Declare(RPCDeclareTransaction::V3(tx)) => tx
    );
    // Truncate the sierra program to trigger an error.
    tx.contract_class.sierra_program = tx.contract_class.sierra_program[..100].to_vec();
    let declare_tx = RPCDeclareTransaction::V3(tx);

    let result = compile_contract_class(&declare_tx);
    assert_matches!(
        result.unwrap_err(),
        GatewayError::CompilationError(CompilationUtilError::AllowedLibfuncsError(
            AllowedLibfuncsError::SierraProgramError
        ))
    )
}

#[test]
fn test_compile_contract_class() {
    let declare_tx = assert_matches!(
        declare_tx(),
        RPCTransaction::Declare(declare_tx) => declare_tx
    );
    let RPCDeclareTransaction::V3(declare_tx_v3) = &declare_tx;
    let contract_class = &declare_tx_v3.contract_class;

    let class_info = compile_contract_class(&declare_tx).unwrap();
    assert_matches!(class_info.contract_class(), ContractClass::V1(_));
    assert_eq!(class_info.sierra_program_length(), contract_class.sierra_program.len());
    assert_eq!(class_info.abi_length(), contract_class.abi.len());
}
