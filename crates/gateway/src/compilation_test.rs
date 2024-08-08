use assert_matches::assert_matches;
use blockifier::execution::contract_class::ContractClass;
use cairo_lang_sierra_to_casm::compiler::CompilationError;
use cairo_lang_starknet_classes::allowed_libfuncs::AllowedLibfuncsError;
use cairo_lang_starknet_classes::casm_contract_class::{
    CasmContractClass,
    StarknetSierraCompilationError,
};
use mempool_test_utils::starknet_api_test_utils::declare_tx as rpc_declare_tx;
use rstest::{fixture, rstest};
use starknet_api::core::CompiledClassHash;
use starknet_api::rpc_transaction::{
    RpcDeclareTransaction,
    RpcDeclareTransactionV3,
    RpcTransaction,
};
use starknet_sierra_compile::config::SierraToCasmCompilationConfig;
use starknet_sierra_compile::errors::CompilationUtilError;
use starknet_sierra_compile::SierraToCasmCompiler;

use crate::compilation::GatewayCompiler;
use crate::config::{GatewayCompilerConfig, PostCompilationConfig};
use crate::errors::GatewayError;

#[fixture]
fn gateway_compiler() -> GatewayCompiler {
    GatewayCompiler::new_cairo_lang_compiler(GatewayCompilerConfig::default())
}

#[fixture]
fn declare_tx_v3() -> RpcDeclareTransactionV3 {
    assert_matches!(
        rpc_declare_tx(),
        RpcTransaction::Declare(RpcDeclareTransaction::V3(declare_tx)) => declare_tx
    )
}

// TODO(Arni): Redesign this test once the compiler is passed with dependancy injection.
// DO that too.
#[rstest]
fn test_compile_contract_class_compiled_class_hash_mismatch(
    gateway_compiler: GatewayCompiler,
    mut declare_tx_v3: RpcDeclareTransactionV3,
) {
    let expected_hash = declare_tx_v3.compiled_class_hash;
    let wrong_supplied_hash = CompiledClassHash::default();
    declare_tx_v3.compiled_class_hash = wrong_supplied_hash;
    let declare_tx = RpcDeclareTransaction::V3(declare_tx_v3);

    let result = gateway_compiler.process_declare_tx(&declare_tx);
    assert_matches!(
        result.unwrap_err(),
        GatewayError::CompiledClassHashMismatch { supplied, hash_result }
        if supplied == wrong_supplied_hash && hash_result == expected_hash
    );
}

// TODO(Arni): Redesign this test once the compiler is passed with dependancy injection.
// TODO(Do that too).
#[rstest]
fn test_compile_contract_class_bytecode_size_validation(declare_tx_v3: RpcDeclareTransactionV3) {
    let sierra_to_casm_compiler_config = SierraToCasmCompilationConfig { max_bytecode_size: 1 };
    let gateway_compiler = GatewayCompiler::new_cairo_lang_compiler(GatewayCompilerConfig {
        sierra_to_casm_compiler_config,
        ..Default::default()
    });

    let result = gateway_compiler.process_declare_tx(&RpcDeclareTransaction::V3(declare_tx_v3));
    assert_matches!(
        result.unwrap_err(),
        GatewayError::CompilationError(CompilationUtilError::StarknetSierraCompilationError(
            StarknetSierraCompilationError::CompilationError(err)
        ))
        if matches!(err.as_ref(), CompilationError::CodeSizeLimitExceeded)
    )
}

#[rstest]
fn test_compile_contract_class_raw_class_size_validation(declare_tx_v3: RpcDeclareTransactionV3) {
    struct GatewayCompilerForTesting;
    impl SierraToCasmCompiler for GatewayCompilerForTesting {
        fn compile(
            &self,
            _contract_class: cairo_lang_starknet_classes::contract_class::ContractClass,
        ) -> Result<CasmContractClass, CompilationUtilError> {
            Ok(CasmContractClass::default())
        }
    }

    let gateway_compiler = GatewayCompiler {
        config: PostCompilationConfig { max_raw_casm_class_size: 1 },
        sierra_to_casm_compiler: std::sync::Arc::new(GatewayCompilerForTesting),
    };

    let result = gateway_compiler.process_declare_tx(&RpcDeclareTransaction::V3(declare_tx_v3));
    assert_matches!(result.unwrap_err(), GatewayError::CasmContractClassObjectSizeTooLarge { .. })
}

#[rstest]
fn test_compile_contract_class_bad_sierra(
    gateway_compiler: GatewayCompiler,
    mut declare_tx_v3: RpcDeclareTransactionV3,
) {
    // Truncate the sierra program to trigger an error.
    declare_tx_v3.contract_class.sierra_program =
        declare_tx_v3.contract_class.sierra_program[..100].to_vec();
    let declare_tx = RpcDeclareTransaction::V3(declare_tx_v3);

    let result = gateway_compiler.process_declare_tx(&declare_tx);
    assert_matches!(
        result.unwrap_err(),
        GatewayError::CompilationError(CompilationUtilError::AllowedLibfuncsError(
            AllowedLibfuncsError::SierraProgramError
        ))
    )
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
    assert_matches!(class_info.contract_class(), ContractClass::V1(_));
    assert_eq!(class_info.sierra_program_length(), sierra_program_length);
    assert_eq!(class_info.abi_length(), abi_length);
}
