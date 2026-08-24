use rstest::rstest;
use starknet_types_core::felt::Felt;

use super::{deserialize_retdata, RetdataDeserializationError, TryFromIterator};

// Consumes exactly two felts, so retdata length mismatches are observable in both directions.
#[derive(Debug, PartialEq, Eq)]
struct FeltPair {
    first: Felt,
    second: Felt,
}

impl TryFromIterator<Felt> for FeltPair {
    type Error = RetdataDeserializationError;

    fn try_from_iter<T: Iterator<Item = Felt>>(iter: &mut T) -> Result<Self, Self::Error> {
        Ok(Self { first: Felt::try_from_iter(iter)?, second: Felt::try_from_iter(iter)? })
    }
}

#[test]
fn deserialize_retdata_accepts_exact_length() {
    assert_eq!(
        deserialize_retdata::<FeltPair>(vec![Felt::ONE, Felt::TWO]).unwrap(),
        FeltPair { first: Felt::ONE, second: Felt::TWO }
    );
}

#[rstest]
#[case::one_leftover_felt(vec![Felt::ONE, Felt::TWO, Felt::THREE])]
#[case::two_leftover_felts(vec![Felt::ONE, Felt::TWO, Felt::THREE, Felt::ZERO])]
fn deserialize_retdata_rejects_leftover_felts(#[case] retdata: Vec<Felt>) {
    let error = deserialize_retdata::<FeltPair>(retdata).unwrap_err();
    assert_eq!(error.to_string(), "Invalid object length: unconsumed elements in retdata.");
}

#[rstest]
#[case::empty_retdata(vec![])]
#[case::one_felt_short(vec![Felt::ONE])]
fn deserialize_retdata_rejects_short_retdata(#[case] retdata: Vec<Felt>) {
    let error = deserialize_retdata::<FeltPair>(retdata).unwrap_err();
    assert!(
        matches!(
            &error,
            RetdataDeserializationError::InvalidObjectLength { message }
                if message.contains("missing felt value")
        ),
        "unexpected error: {error}"
    );
}
