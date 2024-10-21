use starknet_api::felt;
use starknet_api::transaction::Calldata;

use crate::test_utils::{create_calldata, CompilerBasedVersion};

/// Returns the calldata for N recursive call contract syscalls, where N is the length of versions.
/// versions determines the cairo version of the called contract in each recursive call. Final call
/// is a simple local contract call (test_storage_read_write).
/// The first element in the returned value is the calldata for a call from a contract of the first
/// element in versions, to the a contract of the second element, etc.
pub fn build_recurse_calldata(versions: &[CompilerBasedVersion]) -> Calldata {
    if versions.is_empty() {
        return Calldata(vec![].into());
    }
    let last_version = versions.last().unwrap();
    let mut calldata = create_calldata(
        last_version.get_test_contract().get_instance_address(0),
        "test_storage_read_write",
        &[
            felt!(123_u16), // Calldata: address.
            felt!(45_u8),   // Calldata: value.
        ],
    );

    for version in versions[..versions.len() - 1].iter().rev() {
        calldata = create_calldata(
            version.get_test_contract().get_instance_address(0),
            "test_call_contract",
            &calldata.0,
        );
    }
    calldata
}
