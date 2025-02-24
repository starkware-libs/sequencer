use assert_matches::assert_matches;
use indexmap::IndexMap;
use pretty_assertions::assert_eq;
use starknet_api::block::BlockHash;
use starknet_api::core::{CompiledClassHash, Nonce};
use starknet_api::hash::StarkHash;
use starknet_api::transaction::TransactionOffsetInBlock;
use starknet_api::{class_hash, contract_address, felt, storage_key, tx_hash};

use super::{Block, GlobalRoot, TransactionReceiptsError};
use crate::reader::objects::block::BlockPostV0_13_1;
use crate::reader::objects::state::{
    DeclaredClassHashEntry,
    DeployedContract,
    ReplacedClass,
    StateDiff,
    StateUpdate,
    StorageEntry,
};
use crate::reader::objects::transaction::TransactionReceipt;
use crate::reader::ReaderClientError;
use crate::test_utils::read_resource::read_resource_file;

#[test]
fn load_block_succeeds() {
    // TODO(Tzahi): Replace block_post_0_13_3 (copied from 0_13_2 and added additional fields) with
    // live data once available.
    for block_path in [
        "reader/block_post_0_14_0.json",
        "reader/block_post_0_13_4.json",
        "reader/block_post_0_13_3.json",
        "reader/block_post_0_13_2.json",
        "reader/block_post_0_13_1.json",
        "reader/old_block_post_0_13_1_no_sn_version.json",
        "reader/old_block_post_0_13_1_no_sequencer.json",
    ] {
        serde_json::from_str::<Block>(&read_resource_file(block_path)).unwrap_or_else(|err| {
            panic!("Failed loading block in path {block_path}. Error: {err}")
        });
    }
}

#[test]
fn load_block_state_update_succeeds() {
    let expected_state_update = StateUpdate {
        block_hash: BlockHash(felt!(
            "0x3f65ef25e87a83d92f32f5e4869a33580f9db47ec980c1ff27bdb5151914de5"
        )),
        new_root: GlobalRoot(StarkHash::from_hex_unchecked(
            "02ade8eea6eb6523d22a408a1f035bd351a9a5dce28926ca92d7abb490c0e74a",
        )),
        old_root: GlobalRoot(StarkHash::from_hex_unchecked(
            "0465b219d93bcb2776aa3abb009423be3e2d04dba6453d7e027830740cd699a4",
        )),
        state_diff: StateDiff {
            storage_diffs: IndexMap::from([(
                contract_address!(
                    "0x13386f165f065115c1da38d755be261023c32f0134a03a8e66b6bb1e0016014"
                ),
                vec![
                    StorageEntry {
                        key: storage_key!(
                            "0x3b3a699bb6ef37ff4b9c4e14319c7d8e9c9bdd10ff402d1ebde18c62ae58381"
                        ),
                        value: felt!("0x61454dd6e5c83621e41b74c"),
                    },
                    StorageEntry {
                        key: storage_key!(
                            "0x1557182e4359a1f0c6301278e8f5b35a776ab58d39892581e357578fb287836"
                        ),
                        value: felt!("0x79dd8085e3e5a96ea43e7d"),
                    },
                ],
            )]),
            deployed_contracts: vec![DeployedContract {
                address: contract_address!(
                    "0x3e10411edafd29dfe6d427d03e35cb261b7a5efeee61bf73909ada048c029b9"
                ),
                class_hash: class_hash!(
                    "0x071c3c99f5cf76fc19945d4b8b7d34c7c5528f22730d56192b50c6bbfd338a64"
                ),
            }],
            declared_classes: vec![DeclaredClassHashEntry {
                class_hash: class_hash!("0x10"),
                compiled_class_hash: CompiledClassHash(felt!("0x1000")),
            }],
            old_declared_contracts: vec![class_hash!("0x100")],
            nonces: IndexMap::from([(
                contract_address!(
                    "0x51c62af8919b31499b36bd1f1f702c8ef5a6309554427186c7bd456b862c115"
                ),
                Nonce(felt!("0x12")),
            )]),
            replaced_classes: vec![ReplacedClass {
                address: contract_address!(
                    "0x56b0efe9d91fcda0f341af928404056c5220ee0ccc66be15d20611a172dbd52"
                ),
                class_hash: class_hash!(
                    "0x2248aff260e5837317641ff4f861495dd71e78b9dae98a31113e569b336bd26"
                ),
            }],
        },
    };
    assert_eq!(
        expected_state_update,
        serde_json::from_str::<StateUpdate>(&read_resource_file("reader/block_state_update.json"))
            .unwrap()
    )
}

#[tokio::test]
async fn to_starknet_api_block_and_version() {
    let raw_block = read_resource_file("reader/block_post_0_13_2.json");
    let block: Block = serde_json::from_str(&raw_block).unwrap();
    let expected_num_of_tx_outputs = block.transactions().len();
    let starknet_api_block = block.to_starknet_api_block_and_version().unwrap();
    assert_eq!(expected_num_of_tx_outputs, starknet_api_block.body.transaction_outputs.len());

    let mut err_block: BlockPostV0_13_1 = serde_json::from_str(&raw_block).unwrap();
    err_block.transaction_receipts.pop();
    let err = err_block.to_starknet_api_block_and_version().unwrap_err();
    assert_matches!(
        err,
        ReaderClientError::TransactionReceiptsError(
            TransactionReceiptsError::WrongNumberOfReceipts { .. }
        )
    );

    let mut err_block: BlockPostV0_13_1 = serde_json::from_str(&raw_block).unwrap();
    err_block.transaction_receipts[0].transaction_index = TransactionOffsetInBlock(1);
    let err = err_block.to_starknet_api_block_and_version().unwrap_err();
    assert_matches!(
        err,
        ReaderClientError::TransactionReceiptsError(
            TransactionReceiptsError::MismatchTransactionIndex { .. }
        )
    );

    let mut err_block: BlockPostV0_13_1 = serde_json::from_str(&raw_block).unwrap();
    err_block.transaction_receipts[0].transaction_hash = tx_hash!(0x4);
    let err = err_block.to_starknet_api_block_and_version().unwrap_err();
    assert_matches!(
        err,
        ReaderClientError::TransactionReceiptsError(
            TransactionReceiptsError::MismatchTransactionHash { .. }
        )
    );

    let mut err_block: BlockPostV0_13_1 = serde_json::from_str(&raw_block).unwrap();
    err_block.transaction_receipts[0] = TransactionReceipt {
        transaction_hash: err_block.transactions[1].transaction_hash(),
        ..err_block.transaction_receipts[0].clone()
    };
    let err = err_block.to_starknet_api_block_and_version().unwrap_err();
    assert_matches!(
        err,
        ReaderClientError::TransactionReceiptsError(
            TransactionReceiptsError::MismatchTransactionHash { .. }
        )
    );
}

#[tokio::test]
async fn to_starknet_api_block_and_version_0_13_1() {
    let raw_block = read_resource_file("reader/block_post_0_13_1.json");
    let block: Block = serde_json::from_str(&raw_block).unwrap();
    let expected_num_of_tx_outputs = block.transactions().len();
    let starknet_api_block = block.to_starknet_api_block_and_version().unwrap();
    assert_eq!(expected_num_of_tx_outputs, starknet_api_block.body.transaction_outputs.len());
    // Check that for pre 0.13.2 blocks, we erase their hash since it's a deprecated formula.
    assert!(starknet_api_block.header.event_commitment.is_none());
    assert!(starknet_api_block.header.transaction_commitment.is_none());
}
