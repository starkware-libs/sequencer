use serde_json::json;

use super::{verify_block_signature, StarknetVersion};
use crate::block::{BlockHash, BlockNumber, BlockSignature};
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
fn test_vec_version() {
    assert_eq!(StarknetVersion::default().to_string(), "0.0.0");

    let version_123 = StarknetVersion::try_from("1.2.3".to_owned()).unwrap();
    assert_eq!(version_123, StarknetVersion(vec![1, 2, 3]));

    let serialized_123 = json!(version_123);
    assert_eq!(serialized_123, "1.2.3".to_owned());
    assert_eq!(serde_json::from_value::<StarknetVersion>(serialized_123).unwrap(), version_123);

    assert!(StarknetVersion(vec![0, 10, 0]) > StarknetVersion(vec![0, 2, 5]));
    assert!(StarknetVersion(vec![0, 13, 1]) > StarknetVersion(vec![0, 12, 2]));
    assert!(StarknetVersion(vec![0, 13, 0, 1]) > StarknetVersion(vec![0, 13, 0]));
    assert!(StarknetVersion(vec![0, 13, 0]) > StarknetVersion(vec![0, 13]));
}
