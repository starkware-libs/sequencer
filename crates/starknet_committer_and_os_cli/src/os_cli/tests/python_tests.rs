use std::collections::{HashMap, HashSet};

// TODO(Amos): When available in the VM crate, use an existing set, instead of using each hint
//   const explicitly.
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
use cairo_vm::hint_processor::builtin_hint_processor::secp::cairo0_hints::CAIRO0_HINT_CODES;
use starknet_os::hints::enum_definition::{AggregatorHint, HintExtension, OsHint};
use starknet_os::hints::types::HintEnum;
use starknet_os::test_utils::cairo_runner::EntryPointRunnerConfig;
use strum::IntoEnumIterator;

use crate::os_cli::commands::{validate_input, Input};
use crate::os_cli::tests::aliases::aliases_test;
use crate::os_cli::tests::bls_field::test_bls_field;
use crate::os_cli::tests::types::{OsPythonTestError, OsPythonTestResult, OsSpecificTestError};
use crate::os_cli::tests::utils::test_cairo_function;
use crate::shared_utils::types::{PythonTestError, PythonTestRunner};

// Enum representing different Python tests.
pub enum OsPythonTestRunner {
    AliasesTest,
    BlsFieldTest,
    CompareOsHints,
    InputDeserialization,
    RunDummyFunction,
}

// Implements conversion from a string to the test runner.
impl TryFrom<String> for OsPythonTestRunner {
    type Error = OsPythonTestError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "aliases_test" => Ok(Self::AliasesTest),
            "bls_field_test" => Ok(Self::BlsFieldTest),
            "compare_os_hints" => Ok(Self::CompareOsHints),
            "input_deserialization" => Ok(Self::InputDeserialization),
            "run_dummy_function" => Ok(Self::RunDummyFunction),
            _ => Err(PythonTestError::UnknownTestName(value)),
        }
    }
}

impl PythonTestRunner for OsPythonTestRunner {
    type SpecificError = OsSpecificTestError;
    async fn run(&self, input: Option<&str>) -> OsPythonTestResult {
        match self {
            Self::AliasesTest => aliases_test(Self::non_optional_input(input)?),
            Self::BlsFieldTest => test_bls_field(Self::non_optional_input(input)?),
            Self::CompareOsHints => compare_os_hints(Self::non_optional_input(input)?),
            Self::InputDeserialization => input_deserialization(Self::non_optional_input(input)?),
            Self::RunDummyFunction => run_dummy_cairo_function(Self::non_optional_input(input)?),
        }
    }
}

fn compare_os_hints(input: &str) -> OsPythonTestResult {
    let unfiltered_python_hints: HashSet<String> = serde_json::from_str(input)?;

    // Remove VM hints.
    let vm_hints = vm_hints();
    let python_os_hints: HashSet<String> = unfiltered_python_hints
        .into_iter()
        .filter(|hint| !vm_hints.contains(hint.as_str()))
        .collect();

    // We ignore `SyscallHint`s here, as they are not part of the compiled OS.
    let rust_os_hints: HashSet<String> = OsHint::iter()
        .map(|hint| hint.to_str().to_string())
        .chain(HintExtension::iter().map(|hint| hint.to_str().to_string()))
        .chain(AggregatorHint::iter().map(|hint| hint.to_str().to_string()))
        .collect();

    let mut only_in_python: Vec<String> =
        python_os_hints.difference(&rust_os_hints).cloned().collect();
    only_in_python.sort();
    let mut only_in_rust: Vec<String> =
        rust_os_hints.difference(&python_os_hints).cloned().collect();
    only_in_rust.sort();
    Ok(serde_json::to_string(&(only_in_python, only_in_rust))?)
}

// TODO(Amos): Delete this test, as the cairo runner now has it's own separate test.
fn run_dummy_cairo_function(input: &str) -> OsPythonTestResult {
    let param_1 = 123;
    let param_2 = 456;
    test_cairo_function(
        &EntryPointRunnerConfig::default(),
        input,
        "dummy_function",
        &[param_1.into(), param_2.into()],
        &[],
        &[(789 + param_1).into(), param_1.into(), param_2.into()],
        &[],
        HashMap::new(),
    )
}

/// Deserialize the input string into an `Input` struct.
fn input_deserialization(input_str: &str) -> OsPythonTestResult {
    let input = serde_json::from_str::<Input>(input_str)?;
    validate_input(&input.os_hints.os_input);
    Ok("Deserialization successful".to_string())
}

fn vm_hints() -> HashSet<&'static str> {
    let mut vm_hints = HashSet::from([
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
        // TODO(Amos): Load These hints from the cairo VM once the workspace version is upgraded to
        // v2.0.0.
        r#"from starkware.starknet.core.os.data_availability.bls_utils import BLS_PRIME, pack, split

a = pack(ids.a, PRIME)
b = pack(ids.b, PRIME)

q, r = divmod(a * b, BLS_PRIME)

# By the assumption: |a|, |b| < 2**104 * ((2**86) ** 2 + 2**86 + 1) < 2**276.001.
# Therefore |q| <= |ab| / BLS_PRIME < 2**299.
# Hence the absolute value of the high limb of split(q) < 2**127.
segments.write_arg(ids.q.address_, split(q))
segments.write_arg(ids.res.address_, split(r))"#,
        // This hint was modified to reflect changes in the Python crate.
        r#"from starkware.cairo.common.cairo_secp.secp256r1_utils import SECP256R1_ALPHA, SECP256R1_P
from starkware.cairo.common.cairo_secp.secp_utils import pack
from starkware.python.math_utils import ec_double_slope

# Compute the slope.
x = pack(ids.point.x, PRIME)
y = pack(ids.point.y, PRIME)
value = slope = ec_double_slope(point=(x, y), alpha=SECP256R1_ALPHA, p=SECP256R1_P)"#,
        // This hint was modified to reflect changes in the Python crate.
        r#"from starkware.cairo.common.cairo_secp.secp256r1_utils import SECP256R1_P
from starkware.cairo.common.cairo_secp.secp_utils import pack

slope = pack(ids.slope, PRIME)
x = pack(ids.point.x, PRIME)
y = pack(ids.point.y, PRIME)

value = new_x = (pow(slope, 2, SECP256R1_P) - 2 * x) % SECP256R1_P"#,
        // This hint was modified to reflect changes in the Python crate.
        r#"from starkware.cairo.common.cairo_secp.secp_utils import SECP256R1, pack
from starkware.python.math_utils import y_squared_from_x

y_square_int = y_squared_from_x(
    x=pack(ids.x, PRIME),
    alpha=SECP256R1.alpha,
    beta=SECP256R1.beta,
    field_prime=SECP256R1.prime,
)

# Note that (y_square_int ** ((SECP256R1.prime + 1) / 4)) ** 2 =
#   = y_square_int ** ((SECP256R1.prime + 1) / 2) =
#   = y_square_int ** ((SECP256R1.prime - 1) / 2 + 1) =
#   = y_square_int * y_square_int ** ((SECP256R1.prime - 1) / 2) = y_square_int * {+/-}1.
y = pow(y_square_int, (SECP256R1.prime + 1) // 4, SECP256R1.prime)

# We need to decide whether to take y or prime - y.
if ids.v % 2 == y % 2:
    value = y
else:
    value = (-y) % SECP256R1.prime"#,
    ]);
    vm_hints.extend(CAIRO0_HINT_CODES.iter().map(|hint| hint.1));
    vm_hints
}
