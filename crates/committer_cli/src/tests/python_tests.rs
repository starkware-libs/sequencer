use committer::felt::Felt;
use committer::hash::hash_trait::{HashFunction, HashInputPair};
use committer::hash::pedersen::PedersenHashFunction;
use std::collections::HashMap;
use thiserror;

// Enum representing different Python tests.
pub(crate) enum PythonTest {
    ExampleTest,
    FeltSerialize,
    HashFunction,
}

/// Error type for PythonTest enum.
#[derive(Debug, thiserror::Error)]
pub(crate) enum PythonTestError {
    #[error("Unknown test name: {0}")]
    UnknownTestName(String),
    #[error("Failed to parse input: {0}")]
    ParseInputError(#[from] serde_json::Error),
    #[error("Failed to parse integer input: {0}")]
    ParseIntError(#[from] std::num::ParseIntError),
}

/// Implements conversion from a string to a `PythonTest`.
impl TryFrom<String> for PythonTest {
    type Error = PythonTestError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "example_test" => Ok(Self::ExampleTest),
            "felt_serialize_test" => Ok(Self::FeltSerialize),
            "hash_function_test" => Ok(Self::HashFunction),
            _ => Err(PythonTestError::UnknownTestName(value)),
        }
    }
}

impl PythonTest {
    /// Runs the test with the given arguments.
    pub(crate) fn run(&self, input: &str) -> Result<String, PythonTestError> {
        match self {
            Self::ExampleTest => {
                let example_input: HashMap<String, String> = serde_json::from_str(input)?;
                Ok(example_test(example_input))
            }
            Self::FeltSerialize => {
                let felt = input.parse::<u128>()?;
                Ok(felt_serialize_test(felt))
            }
            Self::HashFunction => {
                let hash_input: HashMap<String, u128> = serde_json::from_str(input)?;
                Ok(test_hash_function(hash_input))
            }
        }
    }
}

pub(crate) fn example_test(test_args: HashMap<String, String>) -> String {
    let x = test_args.get("x").expect("Failed to get value for key 'x'");
    let y = test_args.get("y").expect("Failed to get value for key 'y'");
    format!("Calling example test with args: x: {}, y: {}", x, y)
}

/// Serializes a Felt into a string.
pub(crate) fn felt_serialize_test(felt: u128) -> String {
    let bytes = Felt::from(felt).as_bytes().to_vec();
    serde_json::to_string(&bytes)
        .unwrap_or_else(|error| panic!("Failed to serialize felt: {}", error))
}

pub(crate) fn test_hash_function(hash_input: HashMap<String, u128>) -> String {
    // Fetch x and y from the input.
    let x = hash_input
        .get("x")
        .expect("Failed to get value for key 'x'");
    let y = hash_input
        .get("y")
        .expect("Failed to get value for key 'y'");

    // Convert x and y to Felt.
    let x_felt = Felt::from(*x);
    let y_felt = Felt::from(*y);

    // Compute the hash.
    let hash_result = PedersenHashFunction::compute_hash(HashInputPair(x_felt, y_felt)).0;

    // Serialize the hash result.
    serde_json::to_string(&hash_result)
        .unwrap_or_else(|error| panic!("Failed to serialize hash result: {}", error))
}
