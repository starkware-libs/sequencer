use crate::program_hash::compute_os_program_hash;
use crate::PROGRAM_HASH;

#[test]
fn test_program_hash() {
    assert_eq!(compute_os_program_hash().unwrap(), PROGRAM_HASH.os)
}
