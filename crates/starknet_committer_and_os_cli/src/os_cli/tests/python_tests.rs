use std::collections::HashSet;

// TODO(Amos): When available in the VM crate, use an existing set, instead of using each hint
//   const explicitly.
use cairo_vm::hint_processor::builtin_hint_processor::hint_code::HINT_CODES;
use cairo_vm::hint_processor::builtin_hint_processor::kzg_da::WRITE_DIVMOD_SEGMENT;
use cairo_vm::hint_processor::builtin_hint_processor::secp::cairo0_hints::CAIRO0_HINT_CODES;
use starknet_os::hints::enum_definition::{AggregatorHint, HintExtension, OsHint};
use starknet_os::hints::types::HintEnum;
use strum::IntoEnumIterator;

use crate::os_cli::commands::{validate_input, Input};
use crate::os_cli::tests::aliases::aliases_test;
use crate::os_cli::tests::bls_field::test_bls_field;
use crate::os_cli::tests::types::{OsPythonTestError, OsPythonTestResult, OsSpecificTestError};
use crate::shared_utils::types::{PythonTestError, PythonTestRunner};

// Enum representing different Python tests.
pub enum OsPythonTestRunner {
    AliasesTest,
    BlsFieldTest,
    CompareOsHints,
    InputDeserialization,
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

/// Deserialize the input string into an `Input` struct.
fn input_deserialization(input_str: &str) -> OsPythonTestResult {
    let input = serde_json::from_str::<Input>(input_str)?;
    validate_input(&input.os_hints.os_input);
    Ok("Deserialization successful".to_string())
}

fn vm_hints() -> HashSet<&'static str> {
    let mut vm_hints = HashSet::from([
        // TODO(Amos): Use VM hints once they match the updated hints in the Python repo.
        WRITE_DIVMOD_SEGMENT,
        // This hint was modified to reflect changes in the Python repo.
        r#"from starkware.cairo.common.cairo_secp.secp256r1_utils import SECP256R1_ALPHA, SECP256R1_P
from starkware.cairo.common.cairo_secp.secp_utils import pack
from starkware.python.math_utils import ec_double_slope

# Compute the slope.
x = pack(ids.point.x, PRIME)
y = pack(ids.point.y, PRIME)
value = slope = ec_double_slope(point=(x, y), alpha=SECP256R1_ALPHA, p=SECP256R1_P)"#,
        // This hint was modified to reflect changes in the Python repo.
        r#"from starkware.cairo.common.cairo_secp.secp256r1_utils import SECP256R1_P
from starkware.cairo.common.cairo_secp.secp_utils import pack

slope = pack(ids.slope, PRIME)
x = pack(ids.point.x, PRIME)
y = pack(ids.point.y, PRIME)

value = new_x = (pow(slope, 2, SECP256R1_P) - 2 * x) % SECP256R1_P"#,
        // This hint was modified to reflect changes in the Python repo.
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
    vm_hints.extend(HINT_CODES.values());
    vm_hints.extend(CAIRO0_HINT_CODES.values());
    vm_hints
}
