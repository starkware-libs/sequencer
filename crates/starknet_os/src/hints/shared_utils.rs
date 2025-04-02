use std::sync::LazyLock;

use num_bigint::BigInt;
use num_traits::Signed;
use starknet_types_core::felt::Felt;

use crate::hints::error::OsHintError;

pub static BASE: LazyLock<BigInt> = LazyLock::new(|| BigInt::from(1u128 << 86));

/// Takes an integer and returns its canonical representation as:
///    d0 + d1 * BASE + d2 * BASE**2.
/// d2 can be in the range (-2**127, 2**127).
// TODO(Dori): Consider using bls_split from the VM crate if and when public.
pub fn split_bigint3(num: BigInt) -> Result<[Felt; 3], OsHintError> {
    let (q1, d0) = (&num / &*BASE, Felt::from(num % &*BASE));
    let (d2, d1) = (&q1 / &*BASE, Felt::from(q1 % &*BASE));
    if d2.abs() >= BigInt::from(1_u128 << 127) {
        return Err(OsHintError::AssertionFailed {
            message: format!("Remainder should be in (-2**127, 2**127), got {d2}."),
        });
    }

    Ok([d0, d1, Felt::from(d2)])
}
