use indexmap::indexmap;
use starknet_api::block::{BlockHash, BlockHeader, BlockNumber};
use starknet_api::core::Nonce;
use starknet_api::state::ThinStateDiff;
use starknet_api::{class_hash, compiled_class_hash, contract_address, felt, storage_key};

use super::*;
use crate::header::HeaderStorageWriter;
use crate::state::StateStorageWriter;
use crate::test_utils::get_test_storage;

fn write_test_data(writer: &mut crate::StorageWriter) {
    // Block 0: contract A writes to key K1 and K2, contract B writes to K1.
    let diff0 = ThinStateDiff {
        storage_diffs: indexmap! {
            contract_address!("0xA") => indexmap! {
                storage_key!("0x1") => felt!("0x100"),
                storage_key!("0x2") => felt!("0x200"),
            },
            contract_address!("0xB") => indexmap! {
                storage_key!("0x1") => felt!("0x300"),
            },
        },
        nonces: indexmap! {
            contract_address!("0xA") => Nonce(felt!("0x1")),
            contract_address!("0xB") => Nonce(felt!("0x1")),
        },
        deployed_contracts: indexmap! {
            contract_address!("0xA") => class_hash!("0xC1"),
            contract_address!("0xB") => class_hash!("0xC2"),
        },
        class_hash_to_compiled_class_hash: indexmap! {
            class_hash!("0xC1") => compiled_class_hash!(1_u8),
        },
        ..Default::default()
    };

    // Block 1: contract A writes to K1 again (version 2), contract A nonce bumps.
    let diff1 = ThinStateDiff {
        storage_diffs: indexmap! {
            contract_address!("0xA") => indexmap! {
                storage_key!("0x1") => felt!("0x101"),
            },
        },
        nonces: indexmap! {
            contract_address!("0xA") => Nonce(felt!("0x2")),
        },
        ..Default::default()
    };

    let header0 = BlockHeader { block_hash: BlockHash(felt!("0xBB0")), ..Default::default() };
    let header1 = BlockHeader { block_hash: BlockHash(felt!("0xBB1")), ..Default::default() };

    writer
        .begin_rw_txn()
        .unwrap()
        .append_header(BlockNumber(0), &header0)
        .unwrap()
        .append_state_diff(BlockNumber(0), diff0)
        .unwrap()
        .commit()
        .unwrap();

    writer
        .begin_rw_txn()
        .unwrap()
        .append_header(BlockNumber(1), &header1)
        .unwrap()
        .append_state_diff(BlockNumber(1), diff1)
        .unwrap()
        .commit()
        .unwrap();
}

#[test]
fn table_overview() {
    let ((reader, mut writer), _temp_dir) = get_test_storage();
    write_test_data(&mut writer);

    let overview = measure_table_overview(&reader).unwrap();
    // contract_storage should have 4 entries (3 from block 0 + 1 from block 1).
    let contract_storage_entry =
        overview.tables.iter().find(|t| t.name == "contract_storage").unwrap();
    assert_eq!(contract_storage_entry.entries, 4);

    // nonces should have 3 entries (2 from block 0 + 1 from block 1).
    let nonces_entry = overview.tables.iter().find(|t| t.name == "nonces").unwrap();
    assert_eq!(nonces_entry.entries, 3);
}

#[test]
fn flat_state_and_varint() {
    let ((reader, mut writer), _temp_dir) = get_test_storage();
    write_test_data(&mut writer);

    let (flat_state, varint) = measure_flat_state_and_varint(&reader).unwrap();

    // contract_storage: 4 entries, 3 distinct (A,K1), (A,K2), (B,K1). (A,K1) has 2 versions.
    let contract_storage =
        flat_state.tables.iter().find(|t| t.table_name == "contract_storage").unwrap();
    assert_eq!(contract_storage.total_entries, 4);
    assert_eq!(contract_storage.distinct_keys, 3);
    assert!((contract_storage.avg_versions - 4.0 / 3.0).abs() < 0.01);

    // nonces: 3 entries, 2 distinct addresses (A, B). A has 2 versions.
    let nonces = flat_state.tables.iter().find(|t| t.table_name == "nonces").unwrap();
    assert_eq!(nonces.total_entries, 3);
    assert_eq!(nonces.distinct_keys, 2);

    // deployed_contracts: 2 entries, 2 distinct (A, B). No re-deployments.
    let deployed = flat_state.tables.iter().find(|t| t.table_name == "deployed_contracts").unwrap();
    assert_eq!(deployed.total_entries, 2);
    assert_eq!(deployed.distinct_keys, 2);

    // compiled_class_hash: 1 entry, 1 distinct.
    let compiled =
        flat_state.tables.iter().find(|t| t.table_name == "compiled_class_hash").unwrap();
    assert_eq!(compiled.total_entries, 1);
    assert_eq!(compiled.distinct_keys, 1);

    // Varint: all values are small (< 4 bytes), so fits_4_bytes should be high.
    let contract_varint =
        varint.tables.iter().find(|t| t.table_name == "contract_storage").unwrap();
    assert_eq!(contract_varint.total_values, 4);
    // 0x100, 0x200, 0x300, 0x101 all fit in 2 bytes.
    assert_eq!(contract_varint.fits_4_bytes, 4);
    assert!(contract_varint.savings_pct > 90.0);
}

#[test]
fn version_wrapper_overhead() {
    let ((reader, mut writer), _temp_dir) = get_test_storage();
    write_test_data(&mut writer);

    let overview = measure_table_overview(&reader).unwrap();
    let overhead = measure_version_wrapper_overhead(&overview);
    // Should be > 0 since we wrote nonces (VersionZeroWrapper), headers, etc.
    assert!(overhead > 0);
}

#[test]
fn thin_state_diff_mmap_size() {
    let ((reader, mut writer), _temp_dir) = get_test_storage();
    write_test_data(&mut writer);

    let mmap_size = measure_thin_state_diff_mmap_size(&reader).unwrap();
    // Should be > 0 since we wrote 2 state diffs to mmap.
    assert!(mmap_size > 0);
}

#[test]
fn run_analysis_skip_full_iteration() {
    let ((reader, mut writer), _temp_dir) = get_test_storage();
    write_test_data(&mut writer);

    let config =
        AnalysisConfig { mmap_sample_count: 10, compare_db_path: None, skip_full_iteration: true };
    let report = run_analysis(&reader, &config).unwrap();
    assert!(report.flat_state.is_none());
    assert!(report.varint_felt.is_none());
    assert!(!report.overview.tables.is_empty());
    assert!(report.thin_state_diff_mmap_bytes > 0);
}

#[test]
fn run_analysis_full() {
    let ((reader, mut writer), _temp_dir) = get_test_storage();
    write_test_data(&mut writer);

    let config =
        AnalysisConfig { mmap_sample_count: 10, compare_db_path: None, skip_full_iteration: false };
    let report = run_analysis(&reader, &config).unwrap();
    assert!(report.flat_state.is_some());
    assert!(report.varint_felt.is_some());

    // Verify JSON serialization works.
    let json = serde_json::to_string_pretty(&report).unwrap();
    assert!(json.contains("contract_storage"));
    assert!(json.contains("distinct_keys"));
}
