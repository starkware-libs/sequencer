use std::collections::HashSet;

use cairo_vm::hint_processor::builtin_hint_processor::hint_code::{
    ADD_NO_UINT384_CHECK,
    ADD_SEGMENT,
    ASSERT_250_BITS,
    ASSERT_LE_FELT,
    ASSERT_LE_FELT_EXCLUDED_0,
    ASSERT_LE_FELT_EXCLUDED_1,
    ASSERT_LE_FELT_EXCLUDED_2,
    ASSERT_LE_FELT_V_0_6,
    ASSERT_LE_FELT_V_0_8,
    ASSERT_LT_FELT,
    ASSERT_NN,
    ASSERT_NOT_EQUAL,
    ASSERT_NOT_ZERO,
    A_B_BITAND_1,
    BIGINT_PACK_DIV_MOD,
    BIGINT_SAFE_DIV,
    BIGINT_TO_UINT256,
    BLAKE2S_ADD_UINT256,
    BLAKE2S_ADD_UINT256_BIGEND,
    BLAKE2S_COMPUTE,
    BLAKE2S_FINALIZE,
    BLAKE2S_FINALIZE_V2,
    BLAKE2S_FINALIZE_V3,
    BLOCK_PERMUTATION,
    BLOCK_PERMUTATION_WHITELIST_V1,
    BLOCK_PERMUTATION_WHITELIST_V2,
    CAIRO_KECCAK_FINALIZE_V1,
    CAIRO_KECCAK_FINALIZE_V2,
    CAIRO_KECCAK_INPUT_IS_FULL_WORD,
    CHAINED_EC_OP_RANDOM_EC_POINT,
    COMPARE_BYTES_IN_WORD_NONDET,
    COMPARE_KECCAK_FULL_RATE_IN_BYTES_NONDET,
    COMPUTE_SLOPE_SECP256R1_V1,
    COMPUTE_SLOPE_SECP256R1_V2,
    COMPUTE_SLOPE_V1,
    COMPUTE_SLOPE_V2,
    COMPUTE_SLOPE_WHITELIST,
    DEFAULT_DICT_NEW,
    DICT_NEW,
    DICT_READ,
    DICT_SQUASH_COPY_DICT,
    DICT_SQUASH_UPDATE_PTR,
    DICT_UPDATE,
    DICT_WRITE,
    DIV_MOD_N_PACKED_DIVMOD_EXTERNAL_N,
    DIV_MOD_N_PACKED_DIVMOD_V1,
    DIV_MOD_N_SAFE_DIV,
    DIV_MOD_N_SAFE_DIV_PLUS_ONE,
    DI_BIT,
    EC_DOUBLE_ASSIGN_NEW_X_V1,
    EC_DOUBLE_ASSIGN_NEW_X_V2,
    EC_DOUBLE_ASSIGN_NEW_X_V3,
    EC_DOUBLE_ASSIGN_NEW_X_V4,
    EC_DOUBLE_ASSIGN_NEW_Y,
    EC_DOUBLE_SLOPE_EXTERNAL_CONSTS,
    EC_DOUBLE_SLOPE_V1,
    EC_DOUBLE_SLOPE_V2,
    EC_DOUBLE_SLOPE_V3,
    EC_DOUBLE_SLOPE_V4,
    EC_MUL_INNER,
    EC_NEGATE,
    EC_NEGATE_EMBEDDED_SECP,
    EC_RECOVER_DIV_MOD_N_PACKED,
    EC_RECOVER_PRODUCT_DIV_M,
    EC_RECOVER_PRODUCT_MOD,
    EC_RECOVER_SUB_A_B,
    EXAMPLE_BLAKE2S_COMPRESS,
    EXCESS_BALANCE,
    FAST_EC_ADD_ASSIGN_NEW_X,
    FAST_EC_ADD_ASSIGN_NEW_X_V2,
    FAST_EC_ADD_ASSIGN_NEW_X_V3,
    FAST_EC_ADD_ASSIGN_NEW_Y,
    FIND_ELEMENT,
    GET_FELT_BIT_LENGTH,
    GET_POINT_FROM_X,
    HI_MAX_BITLEN,
    IMPORT_SECP256R1_ALPHA,
    IMPORT_SECP256R1_N,
    IMPORT_SECP256R1_P,
    INV_MOD_P_UINT256,
    INV_MOD_P_UINT512,
    IS_250_BITS,
    IS_ADDR_BOUNDED,
    IS_LE_FELT,
    IS_NN,
    IS_NN_OUT_OF_RANGE,
    IS_POSITIVE,
    IS_QUAD_RESIDUE,
    IS_ZERO_ASSIGN_SCOPE_VARS,
    IS_ZERO_ASSIGN_SCOPE_VARS_ED25519,
    IS_ZERO_ASSIGN_SCOPE_VARS_EXTERNAL_SECP,
    IS_ZERO_INT,
    IS_ZERO_NONDET,
    IS_ZERO_PACK_ED25519,
    IS_ZERO_PACK_EXTERNAL_SECP_V1,
    IS_ZERO_PACK_EXTERNAL_SECP_V2,
    IS_ZERO_PACK_V1,
    IS_ZERO_PACK_V2,
    KECCAK_WRITE_ARGS,
    MEMCPY_CONTINUE_COPYING,
    MEMCPY_ENTER_SCOPE,
    MEMSET_CONTINUE_LOOP,
    MEMSET_ENTER_SCOPE,
    NONDET_BIGINT3_V1,
    NONDET_BIGINT3_V2,
    NONDET_ELEMENTS_OVER_TEN,
    NONDET_ELEMENTS_OVER_TWO,
    NONDET_N_GREATER_THAN_10,
    NONDET_N_GREATER_THAN_2,
    PACK_MODN_DIV_MODN,
    POW,
    QUAD_BIT,
    RANDOM_EC_POINT,
    RECOVER_Y,
    REDUCE_ED25519,
    REDUCE_V1,
    REDUCE_V2,
    RELOCATE_SEGMENT,
    RUN_P_CIRCUIT,
    RUN_P_CIRCUIT_WITH_LARGE_BATCH_SIZE,
    SEARCH_SORTED_LOWER,
    SET_ADD,
    SHA256_FINALIZE,
    SHA256_INPUT,
    SHA256_MAIN_ARBITRARY_INPUT_LENGTH,
    SHA256_MAIN_CONSTANT_INPUT_LENGTH,
    SIGNED_DIV_REM,
    SPLIT_64,
    SPLIT_FELT,
    SPLIT_INPUT_12,
    SPLIT_INPUT_15,
    SPLIT_INPUT_3,
    SPLIT_INPUT_6,
    SPLIT_INPUT_9,
    SPLIT_INT,
    SPLIT_INT_ASSERT_RANGE,
    SPLIT_N_BYTES,
    SPLIT_OUTPUT_0,
    SPLIT_OUTPUT_1,
    SPLIT_OUTPUT_MID_LOW_HIGH,
    SPLIT_XX,
    SQRT,
    SQUARE_SLOPE_X_MOD_P,
    SQUASH_DICT,
    SQUASH_DICT_INNER_ASSERT_LEN_KEYS,
    SQUASH_DICT_INNER_CHECK_ACCESS_INDEX,
    SQUASH_DICT_INNER_CONTINUE_LOOP,
    SQUASH_DICT_INNER_FIRST_ITERATION,
    SQUASH_DICT_INNER_LEN_ASSERT,
    SQUASH_DICT_INNER_NEXT_KEY,
    SQUASH_DICT_INNER_SKIP_LOOP,
    SQUASH_DICT_INNER_USED_ACCESSES_ASSERT,
    SUB_REDUCED_A_AND_REDUCED_B,
    TEMPORARY_ARRAY,
    UINT128_ADD,
    UINT256_ADD,
    UINT256_ADD_LOW,
    UINT256_EXPANDED_UNSIGNED_DIV_REM,
    UINT256_GET_SQUARE_ROOT,
    UINT256_MUL_DIV_MOD,
    UINT256_MUL_INV_MOD_P,
    UINT256_SIGNED_NN,
    UINT256_SQRT,
    UINT256_SQRT_FELT,
    UINT256_SUB,
    UINT256_UNSIGNED_DIV_REM,
    UINT384_DIV,
    UINT384_GET_SQUARE_ROOT,
    UINT384_SIGNED_NN,
    UINT384_SPLIT_128,
    UINT384_SQRT,
    UINT384_UNSIGNED_DIV_REM,
    UINT512_UNSIGNED_DIV_REM,
    UNSAFE_KECCAK,
    UNSAFE_KECCAK_FINALIZE,
    UNSIGNED_DIV_REM,
    UNSIGNED_DIV_REM_UINT768_BY_UINT384,
    UNSIGNED_DIV_REM_UINT768_BY_UINT384_STRIPPED,
    USORT_BODY,
    USORT_ENTER_SCOPE,
    USORT_VERIFY,
    USORT_VERIFY_MULTIPLICITY_ASSERT,
    USORT_VERIFY_MULTIPLICITY_BODY,
    VERIFY_ECDSA_SIGNATURE,
    VERIFY_ZERO_EXTERNAL_SECP,
    VERIFY_ZERO_V1,
    VERIFY_ZERO_V2,
    VERIFY_ZERO_V3,
    VM_ENTER_SCOPE,
    VM_EXIT_SCOPE,
    XS_SAFE_DIV,
};
// when using 2.0.0-rc4
use cairo_vm::hint_processor::builtin_hint_processor::kzg_da::WRITE_DIVMOD_SEGMENT;
use cairo_vm::hint_processor::builtin_hint_processor::secp::cairo0_hints::{
    COMPUTE_IDS_HIGH_LOW,
    COMPUTE_Q_MOD_PRIME,
    COMPUTE_VALUE_DIV_MOD,
    FAST_SECP_ADD_ASSIGN_NEW_Y,
    GENERATE_NIBBLES,
    IS_ON_CURVE_2,
    SECP_DOUBLE_ASSIGN_NEW_X,
    SECP_R1_GET_POINT_FROM_X,
    SECP_REDUCE,
    SECP_REDUCE_X,
    WRITE_NIBBLES_TO_MEM,
};
use regex::Regex;
use starknet_os::hints::enum_definition::{HintExtension, OsHint};
use starknet_os::hints::types::HintEnum;
use strum::IntoEnumIterator;
use strum_macros::Display;
use thiserror;

use crate::shared_utils::types::{PythonTestError, PythonTestResult, PythonTestRunner};

const SUCCESS_RESULT: &str = "SUCCESS";
pub type OsPythonTestError = PythonTestError<OsSpecificTestError>;
type OsPythonTestResult = PythonTestResult<OsSpecificTestError>;

// Enum representing different Python tests.
pub enum OsPythonTestRunner {
    OsHintCompatibility,
}

// Implements conversion from a string to the test runner.
impl TryFrom<String> for OsPythonTestRunner {
    type Error = OsPythonTestError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "os_hint_compatibility_test" => Ok(Self::OsHintCompatibility),
            _ => Err(PythonTestError::UnknownTestName(value)),
        }
    }
}

#[derive(Debug, thiserror::Error, Display)]
pub enum OsSpecificTestError {
    PlaceHolder,
}

impl PythonTestRunner for OsPythonTestRunner {
    type SpecificError = OsSpecificTestError;
    async fn run(&self, input: Option<&str>) -> OsPythonTestResult {
        match self {
            Self::OsHintCompatibility => {
                test_os_hints_are_equal(Self::non_optional_input(input)?).await
            }
        }
    }
}

async fn test_os_hints_are_equal(input: &str) -> OsPythonTestResult {
    let unfiltered_python_hints: HashSet<String> = serde_json::from_str(input)?;

    // Remove VM hints.
    let vm_hints = vm_hints();
    let mut python_os_hints: HashSet<String> = unfiltered_python_hints
        .into_iter()
        .filter(|hint| !vm_hints.contains(hint.as_str()))
        .collect();

    // Remove hints of the form `exit_syscall(<params>)`, as they were replaced by a single hint -
    // `exit_syscall()`.
    let exit_syscall_re = Regex::new(r"exit_syscall\(.+\)").unwrap();
    python_os_hints.retain(|hint| !exit_syscall_re.is_match(hint));

    // TODO(Amos): Remove once TODOs are removed by the Python side.
    // Remove TODOs from hints.
    let todo_re = Regex::new(r"[ \t]*# TODO.*\n").unwrap();
    python_os_hints = python_os_hints
        .into_iter()
        .map(|hint| todo_re.replace_all(&hint, "").to_string())
        .collect();

    let mut rust_os_hints: HashSet<String> = OsHint::iter()
        .map(|hint| hint.to_str().to_string())
        .chain(HintExtension::iter().map(|hint| hint.to_str().to_string()))
        .collect();

    // The corresponding Python hints were removed manually.
    rust_os_hints.remove("exit_syscall()");

    if rust_os_hints != python_os_hints {
        let mut only_in_python: Vec<String> =
            python_os_hints.difference(&rust_os_hints).cloned().collect();
        only_in_python.sort();
        let mut only_in_rust: Vec<String> =
            rust_os_hints.difference(&python_os_hints).cloned().collect();
        only_in_rust.sort();
        println!("len python_os_hints: {}", python_os_hints.len());
        println!("len rust_os_hints: {}", rust_os_hints.len());
        println!("len only_in_python: {}", only_in_python.len());
        println!("len only_in_rust: {}", only_in_rust.len());
        println!("---------------- Only in Python ----------------");
        for (i, hint) in only_in_python.iter().enumerate() {
            // println!("only in Python: #{}\n", i);
            println!("#{}\n", i);
            println!("{}", hint);
            println!("---------");
        }
        println!("---------------- Only in Rust ----------------");
        for (i, hint) in only_in_rust.iter().enumerate() {
            // println!("only in Rust: #{}\n", i);
            println!("#{}\n", i);
            println!("{}", hint);
            println!("---------");
        }
        return Ok(serde_json::to_string(&(only_in_python, only_in_rust))?);
    }
    Ok(serde_json::to_string(SUCCESS_RESULT)?)
}

fn vm_hints() -> HashSet<&'static str> {
    HashSet::from([
        ADD_SEGMENT,
        VM_ENTER_SCOPE,
        VM_EXIT_SCOPE,
        MEMCPY_ENTER_SCOPE,
        MEMCPY_CONTINUE_COPYING,
        MEMSET_ENTER_SCOPE,
        MEMSET_CONTINUE_LOOP,
        POW,
        IS_NN,
        IS_NN_OUT_OF_RANGE,
        IS_LE_FELT,
        IS_POSITIVE,
        ASSERT_NN,
        ASSERT_NOT_ZERO,
        ASSERT_NOT_EQUAL,
        ASSERT_LE_FELT,
        ASSERT_LE_FELT_V_0_6,
        ASSERT_LE_FELT_V_0_8,
        ASSERT_LE_FELT_EXCLUDED_0,
        ASSERT_LE_FELT_EXCLUDED_1,
        ASSERT_LE_FELT_EXCLUDED_2,
        ASSERT_LT_FELT,
        SPLIT_INT_ASSERT_RANGE,
        ASSERT_250_BITS,
        IS_250_BITS,
        IS_ADDR_BOUNDED,
        SPLIT_INT,
        SPLIT_64,
        SPLIT_FELT,
        SQRT,
        UNSIGNED_DIV_REM,
        SIGNED_DIV_REM,
        IS_QUAD_RESIDUE,
        FIND_ELEMENT,
        SEARCH_SORTED_LOWER,
        SET_ADD,
        DEFAULT_DICT_NEW,
        DICT_NEW,
        DICT_READ,
        DICT_WRITE,
        DICT_UPDATE,
        SQUASH_DICT,
        SQUASH_DICT_INNER_SKIP_LOOP,
        SQUASH_DICT_INNER_FIRST_ITERATION,
        SQUASH_DICT_INNER_CHECK_ACCESS_INDEX,
        SQUASH_DICT_INNER_CONTINUE_LOOP,
        SQUASH_DICT_INNER_ASSERT_LEN_KEYS,
        SQUASH_DICT_INNER_LEN_ASSERT,
        SQUASH_DICT_INNER_USED_ACCESSES_ASSERT,
        SQUASH_DICT_INNER_NEXT_KEY,
        DICT_SQUASH_COPY_DICT,
        DICT_SQUASH_UPDATE_PTR,
        BIGINT_TO_UINT256,
        UINT256_ADD,
        UINT256_ADD_LOW,
        UINT128_ADD,
        UINT256_SUB,
        UINT256_SQRT,
        UINT256_SQRT_FELT,
        UINT256_SIGNED_NN,
        UINT256_UNSIGNED_DIV_REM,
        UINT256_EXPANDED_UNSIGNED_DIV_REM,
        UINT256_MUL_DIV_MOD,
        USORT_ENTER_SCOPE,
        USORT_BODY,
        USORT_VERIFY,
        USORT_VERIFY_MULTIPLICITY_ASSERT,
        USORT_VERIFY_MULTIPLICITY_BODY,
        BLAKE2S_COMPUTE,
        BLAKE2S_FINALIZE,
        BLAKE2S_FINALIZE_V2,
        BLAKE2S_FINALIZE_V3,
        BLAKE2S_ADD_UINT256,
        BLAKE2S_ADD_UINT256_BIGEND,
        EXAMPLE_BLAKE2S_COMPRESS,
        NONDET_BIGINT3_V1,
        NONDET_BIGINT3_V2,
        VERIFY_ZERO_V1,
        VERIFY_ZERO_V2,
        VERIFY_ZERO_V3,
        VERIFY_ZERO_EXTERNAL_SECP,
        REDUCE_V1,
        REDUCE_V2,
        REDUCE_ED25519,
        UNSAFE_KECCAK,
        UNSAFE_KECCAK_FINALIZE,
        IS_ZERO_NONDET,
        IS_ZERO_INT,
        IS_ZERO_PACK_V1,
        IS_ZERO_PACK_V2,
        IS_ZERO_PACK_EXTERNAL_SECP_V1,
        IS_ZERO_PACK_EXTERNAL_SECP_V2,
        IS_ZERO_PACK_ED25519,
        IS_ZERO_ASSIGN_SCOPE_VARS,
        IS_ZERO_ASSIGN_SCOPE_VARS_EXTERNAL_SECP,
        IS_ZERO_ASSIGN_SCOPE_VARS_ED25519,
        DIV_MOD_N_PACKED_DIVMOD_V1,
        DIV_MOD_N_PACKED_DIVMOD_EXTERNAL_N,
        DIV_MOD_N_SAFE_DIV,
        GET_FELT_BIT_LENGTH,
        BIGINT_PACK_DIV_MOD,
        BIGINT_SAFE_DIV,
        DIV_MOD_N_SAFE_DIV_PLUS_ONE,
        GET_POINT_FROM_X,
        EC_NEGATE,
        EC_NEGATE_EMBEDDED_SECP,
        EC_DOUBLE_SLOPE_V1,
        EC_DOUBLE_SLOPE_V2,
        EC_DOUBLE_SLOPE_V3,
        EC_DOUBLE_SLOPE_V4,
        EC_DOUBLE_SLOPE_EXTERNAL_CONSTS,
        COMPUTE_SLOPE_V1,
        COMPUTE_SLOPE_V2,
        COMPUTE_SLOPE_SECP256R1_V1,
        COMPUTE_SLOPE_SECP256R1_V2,
        IMPORT_SECP256R1_P,
        COMPUTE_SLOPE_WHITELIST,
        EC_DOUBLE_ASSIGN_NEW_X_V1,
        EC_DOUBLE_ASSIGN_NEW_X_V2,
        EC_DOUBLE_ASSIGN_NEW_X_V3,
        EC_DOUBLE_ASSIGN_NEW_X_V4,
        EC_DOUBLE_ASSIGN_NEW_Y,
        SHA256_INPUT,
        SHA256_MAIN_CONSTANT_INPUT_LENGTH,
        SHA256_MAIN_ARBITRARY_INPUT_LENGTH,
        SHA256_FINALIZE,
        KECCAK_WRITE_ARGS,
        COMPARE_BYTES_IN_WORD_NONDET,
        COMPARE_KECCAK_FULL_RATE_IN_BYTES_NONDET,
        BLOCK_PERMUTATION,
        BLOCK_PERMUTATION_WHITELIST_V1,
        BLOCK_PERMUTATION_WHITELIST_V2,
        CAIRO_KECCAK_INPUT_IS_FULL_WORD,
        CAIRO_KECCAK_FINALIZE_V1,
        CAIRO_KECCAK_FINALIZE_V2,
        FAST_EC_ADD_ASSIGN_NEW_X,
        FAST_EC_ADD_ASSIGN_NEW_X_V2,
        FAST_EC_ADD_ASSIGN_NEW_X_V3,
        FAST_EC_ADD_ASSIGN_NEW_Y,
        EC_MUL_INNER,
        RELOCATE_SEGMENT,
        TEMPORARY_ARRAY,
        VERIFY_ECDSA_SIGNATURE,
        SPLIT_OUTPUT_0,
        SPLIT_OUTPUT_1,
        SPLIT_INPUT_3,
        SPLIT_INPUT_6,
        SPLIT_INPUT_9,
        SPLIT_INPUT_12,
        SPLIT_INPUT_15,
        SPLIT_N_BYTES,
        SPLIT_OUTPUT_MID_LOW_HIGH,
        NONDET_N_GREATER_THAN_10,
        NONDET_N_GREATER_THAN_2,
        RANDOM_EC_POINT,
        CHAINED_EC_OP_RANDOM_EC_POINT,
        RECOVER_Y,
        PACK_MODN_DIV_MODN,
        XS_SAFE_DIV,
        UINT384_UNSIGNED_DIV_REM,
        UINT384_SPLIT_128,
        ADD_NO_UINT384_CHECK,
        UINT384_SQRT,
        SUB_REDUCED_A_AND_REDUCED_B,
        UNSIGNED_DIV_REM_UINT768_BY_UINT384,
        UNSIGNED_DIV_REM_UINT768_BY_UINT384_STRIPPED,
        UINT384_SIGNED_NN,
        IMPORT_SECP256R1_ALPHA,
        IMPORT_SECP256R1_N,
        UINT384_GET_SQUARE_ROOT,
        UINT256_GET_SQUARE_ROOT,
        UINT384_DIV,
        INV_MOD_P_UINT256,
        HI_MAX_BITLEN,
        QUAD_BIT,
        INV_MOD_P_UINT512,
        DI_BIT,
        EC_RECOVER_DIV_MOD_N_PACKED,
        UINT512_UNSIGNED_DIV_REM,
        EC_RECOVER_SUB_A_B,
        A_B_BITAND_1,
        EC_RECOVER_PRODUCT_MOD,
        UINT256_MUL_INV_MOD_P,
        EC_RECOVER_PRODUCT_DIV_M,
        SQUARE_SLOPE_X_MOD_P,
        SPLIT_XX,
        RUN_P_CIRCUIT,
        RUN_P_CIRCUIT_WITH_LARGE_BATCH_SIZE,
        NONDET_ELEMENTS_OVER_TEN,
        NONDET_ELEMENTS_OVER_TWO,
        EXCESS_BALANCE,
        // From cairo0_hints
        COMPUTE_IDS_HIGH_LOW,
        COMPUTE_VALUE_DIV_MOD,
        FAST_SECP_ADD_ASSIGN_NEW_Y,
        GENERATE_NIBBLES,
        IS_ON_CURVE_2,
        SECP_DOUBLE_ASSIGN_NEW_X,
        SECP_R1_GET_POINT_FROM_X,
        SECP_REDUCE,
        SECP_REDUCE_X,
        SECP_REDUCE_X,
        WRITE_NIBBLES_TO_MEM,
        COMPUTE_Q_MOD_PRIME,
        // From kzg_da.mod
        WRITE_DIVMOD_SEGMENT,
    ])
}
