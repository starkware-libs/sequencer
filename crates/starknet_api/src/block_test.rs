use serde_json::json;
use strum::IntoEnumIterator;

use super::{verify_block_signature, StarknetVersion};
use crate::block::{BlockHash, BlockNumber, BlockSignature, BlockTimestamp};
use crate::core::{GlobalRoot, SequencerPublicKey};
use crate::crypto::utils::{PublicKey, Signature};
use crate::felt;

#[test]
fn test_block_number_iteration() {
    let start: u64 = 3;
    let up_until: u64 = 10;

    let mut expected = vec![];
    for i in start..up_until {
        expected.push(BlockNumber(i));
    }

    let start_block_number = BlockNumber(start);
    let up_until_block_number = BlockNumber(up_until);

    let mut from_iter: Vec<_> = vec![];
    for i in start_block_number.iter_up_to(up_until_block_number) {
        from_iter.push(i);
    }

    assert_eq!(expected, from_iter);
}

#[test]
fn block_signature_verification() {
    // Values taken from Mainnet.
    let block_hash =
        BlockHash(felt!("0x7d5db04c5ca2aea828180dc441afb1580e3cee7547a3567ced3aa5bb8b273c0"));
    let state_commitment =
        GlobalRoot(felt!("0x64689c12248e1110af4b3af0e2b43cd51ad13e8855f10e37669e2a4baf919c6"));
    let signature = BlockSignature(Signature {
        r: felt!("0x1b382bbfd693011c9b7692bc932b23ed9c288deb27c8e75772e172abbe5950c"),
        s: felt!("0xbe4438085057e1a7c704a0da3b30f7b8340fe3d24c86772abfd24aa597e42"),
    });
    let sequencer_pub_key = SequencerPublicKey(PublicKey(felt!(
        "0x48253ff2c3bed7af18bde0b611b083b39445959102d4947c51c4db6aa4f4e58"
    )));

    assert!(
        verify_block_signature(&sequencer_pub_key, &signature, &state_commitment, &block_hash)
            .unwrap()
    );
}

#[test]
fn test_version_serde() {
    for version in StarknetVersion::iter() {
        // To/from Vec<u8>.
        assert_eq!(StarknetVersion::try_from(Vec::<u8>::from(&version)).unwrap(), version);
        // To/from json.
        assert_eq!(serde_json::from_value::<StarknetVersion>(json!(version)).unwrap(), version);
    }

    // Sanity check substring deserialization.
    assert_eq!(StarknetVersion::try_from("0.13.1").unwrap(), StarknetVersion::V0_13_1);
    assert_eq!(StarknetVersion::try_from("0.13.1.1").unwrap(), StarknetVersion::V0_13_1_1);
}

/// Order of version variants should match byte-vector lexicographic order.
#[test]
fn test_version_byte_vec_order() {
    let versions = StarknetVersion::iter().collect::<Vec<_>>();
    for i in 0..(versions.len() - 1) {
        assert!(Vec::<u8>::from(versions[i]) <= Vec::<u8>::from(versions[i + 1]));
    }
}

#[test]
fn test_latest_version() {
    let latest = StarknetVersion::LATEST;
    assert_eq!(StarknetVersion::default(), latest);
    for version in StarknetVersion::iter() {
        assert!(version <= latest);
    }
}

#[test]
fn test_block_timestamp_display() {
    let timestamp = BlockTimestamp(1_752_482_544);
    let expected = "2025-07-14 08:42:24 UTC";

    assert_eq!(timestamp.to_string(), expected);
}
