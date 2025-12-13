use std::fmt::Debug;

use crate::state::errors::StateError;
use crate::state::state_api::StateResult;

// Compares the results of class retrieval from state.
// Note: this function does not work for general StateResult comparisons. Add more cases as needed.
pub fn assert_eq_state_result<T: PartialEq + Debug>(a: &StateResult<T>, b: &StateResult<T>) {
    match (a, b) {
        (Ok(a), Ok(b)) => assert_eq!(a, b),
        (Err(StateError::UndeclaredClassHash(a)), Err(StateError::UndeclaredClassHash(b))) => {
            assert_eq!(a, b)
        }
        _ => panic!("StateResult mismatch (or unsupported comparison): {a:?} vs {b:?}"),
    }
}
