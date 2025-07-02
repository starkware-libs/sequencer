use num_traits::ToPrimitive;
use starknet_types_core::felt::Felt;

use super::{N_UPDATES_BOUND, N_UPDATES_SMALL_PACKING_BOUND};
#[test]
fn assert_const_felts() {
    assert_eq!(Into::<Felt>::into(N_UPDATES_BOUND).to_u128().unwrap(), 1 << 64);
    assert_eq!(Into::<Felt>::into(N_UPDATES_SMALL_PACKING_BOUND).to_u64().unwrap(), 1 << 8);
}
