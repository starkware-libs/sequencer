use std::borrow::Cow;

use assert_matches::assert_matches;
use byteorder::{BigEndian, ReadBytesExt};
use tempfile::TempDir;

use super::{CommonPrefix, DupSortTableType, DupSortUtils};
use crate::db::db_test::get_test_env;
use crate::db::serialization::NoVersionValueWrapper;
use crate::db::table_types::simple_table::SimpleTable;
use crate::db::table_types::test_utils::{random_table_test, table_test, TableKey, TableValue};
use crate::db::table_types::{DbCursorTrait, Table};
use crate::db::{DbError, DbReader, DbResult, DbWriter, TableIdentifier};

#[test]
fn common_prefix_table() {
    table_test(DbWriter::create_common_prefix_table);
}

/// Test keys sorted by value: entries that exist in the table and gap keys for probing.
const KEY_BEFORE_ALL: TableKey = (0, 0);
const FIRST_ENTRY: (TableKey, u32) = ((10, 20), 100);
const GAP_WITHIN_MAIN_KEY: TableKey = (10, 30);
const SECOND_ENTRY: (TableKey, u32) = ((10, 50), 200);
const GAP_PAST_LAST_SUB_KEY: TableKey = (10, 60);
const GAP_BETWEEN_MAIN_KEYS: TableKey = (20, 0);
const THIRD_ENTRY: (TableKey, u32) = ((30, 10), 300);
const LAST_ENTRY: (TableKey, u32) = ((30, 40), 400);
const KEY_PAST_ALL: TableKey = (40, 0);
const BOUNDARY_TEST_ENTRIES: [(TableKey, u32); 4] =
    [FIRST_ENTRY, SECOND_ENTRY, THIRD_ENTRY, LAST_ENTRY];

/// Creates a SimpleTable and a CommonPrefix table, both populated with
/// [`BOUNDARY_TEST_ENTRIES`]. Returns the reader and both table identifiers.
#[allow(clippy::type_complexity)]
fn setup_boundary_test_tables() -> (
    DbReader,
    TableIdentifier<TableKey, TableValue, SimpleTable>,
    TableIdentifier<TableKey, TableValue, CommonPrefix>,
    TempDir,
) {
    let ((reader, mut writer), temp_dir) = get_test_env();
    let simple_table_id = writer.create_simple_table::<TableKey, TableValue>("simple").unwrap();
    let common_prefix_table_id =
        writer.create_common_prefix_table::<u32, u32, TableValue>("cp").unwrap();

    let wtxn = writer.begin_rw_txn().unwrap();
    let simple_handle = wtxn.open_table(&simple_table_id).unwrap();
    let common_prefix_handle = wtxn.open_table(&common_prefix_table_id).unwrap();
    for &(key, val) in &BOUNDARY_TEST_ENTRIES {
        simple_handle.upsert(&wtxn, &key, &val).unwrap();
        common_prefix_handle.upsert(&wtxn, &key, &val).unwrap();
    }
    wtxn.commit().unwrap();

    (reader, simple_table_id, common_prefix_table_id, temp_dir)
}

type RawBytes<'a> = Cow<'a, [u8]>;
type RawKV<'a> = (RawBytes<'a>, RawBytes<'a>);

fn decode_simple(kv: &RawKV<'_>) -> ((u32, u32), u32) {
    let mut k = kv.0.as_ref();
    let main_key = k.read_u32::<BigEndian>().unwrap();
    let sub_key = k.read_u32::<BigEndian>().unwrap();
    let mut v = kv.1.as_ref();
    let value = v.read_u32::<BigEndian>().unwrap();
    ((main_key, sub_key), value)
}

fn decode_dupsort(kv: &RawKV<'_>) -> ((u32, u32), u32) {
    let mut k = kv.0.as_ref();
    let main_key = k.read_u32::<BigEndian>().unwrap();
    let mut d = kv.1.as_ref();
    let sub_key = d.read_u32::<BigEndian>().unwrap();
    let value = d.read_u32::<BigEndian>().unwrap();
    ((main_key, sub_key), value)
}

// The following tests intentionally test raw libmdbx cursor behavior. The DUP_SORT
// DbCursorTrait impl relies on this behavior for its EOF compensation logic.
// If libmdbx changes it, these tests flag that the compensation logic needs updating.
//
// After MDBX_NEXT or MDBX_PREV returns NOTFOUND, the cursor enters an EOF/unpositioned
// state. Non-DUP_SORT tables happen to leave get_current() valid after NOTFOUND
// (implementation detail, not a contract). DUP_SORT tables properly enter EOF state.

/// last → next(EOF) → get_current → prev: cursors diverge.
#[test]
fn raw_mdbx_cursor_last_next_eof_prev() {
    let (reader, simple_table_id, common_prefix_table_id, _temp_dir) = setup_boundary_test_tables();
    let rtxn = reader.begin_ro_txn().unwrap();
    let simple_table = rtxn.open_table(&simple_table_id).unwrap();
    let common_prefix_table = rtxn.open_table(&common_prefix_table_id).unwrap();

    let mut simple_db_cursor = simple_table.cursor(&rtxn).unwrap();
    let mut dupsort_db_cursor = common_prefix_table.cursor(&rtxn).unwrap();
    let simple_raw = &mut simple_db_cursor.cursor;
    let dupsort_raw = &mut dupsort_db_cursor.cursor;

    let simple_last = simple_raw.last::<RawBytes<'_>, RawBytes<'_>>().unwrap().unwrap();
    let dupsort_last = dupsort_raw.last::<RawBytes<'_>, RawBytes<'_>>().unwrap().unwrap();
    assert_eq!(decode_simple(&simple_last).0, LAST_ENTRY.0);
    assert_eq!(decode_dupsort(&dupsort_last).0, LAST_ENTRY.0);

    let simple_next = simple_raw.next::<RawBytes<'_>, RawBytes<'_>>().unwrap();
    let dupsort_next = dupsort_raw.next::<RawBytes<'_>, RawBytes<'_>>().unwrap();
    assert!(simple_next.is_none());
    assert!(dupsort_next.is_none());

    // After NOTFOUND, non-DUP_SORT cursor stays at last entry; DUP_SORT enters EOF.
    let simple_current = simple_raw.get_current::<RawBytes<'_>, RawBytes<'_>>().unwrap();
    let dupsort_current = dupsort_raw.get_current::<RawBytes<'_>, RawBytes<'_>>().unwrap();
    assert!(simple_current.is_some(), "non-DUP_SORT: get_current valid after NOTFOUND");
    assert!(dupsort_current.is_none(), "DUP_SORT: get_current returns None after NOTFOUND");

    // prev() diverges: non-DUP_SORT returns second-to-last; DUP_SORT returns a different entry.
    let simple_prev = simple_raw.prev::<RawBytes<'_>, RawBytes<'_>>().unwrap();
    let dupsort_prev = dupsort_raw.prev::<RawBytes<'_>, RawBytes<'_>>().unwrap();
    let simple_prev_decoded = decode_simple(&simple_prev.unwrap());
    assert_eq!(
        simple_prev_decoded.0, THIRD_ENTRY.0,
        "simple: prev from EOF should return second-to-last"
    );
    let dupsort_prev_decoded = decode_dupsort(&dupsort_prev.unwrap());
    assert_ne!(
        simple_prev_decoded.0, dupsort_prev_decoded.0,
        "raw cursors diverge after next-NOTFOUND → prev"
    );
}

/// set_range past all entries → prev: both return LAST_ENTRY.
#[test]
fn raw_mdbx_cursor_set_range_past_end_prev() {
    let (reader, simple_table_id, common_prefix_table_id, _temp_dir) = setup_boundary_test_tables();
    let rtxn = reader.begin_ro_txn().unwrap();
    let simple_table = rtxn.open_table(&simple_table_id).unwrap();
    let common_prefix_table = rtxn.open_table(&common_prefix_table_id).unwrap();

    let mut simple_db_cursor = simple_table.cursor(&rtxn).unwrap();
    let mut dupsort_db_cursor = common_prefix_table.cursor(&rtxn).unwrap();
    let simple_raw = &mut simple_db_cursor.cursor;
    let dupsort_raw = &mut dupsort_db_cursor.cursor;

    let past_key_simple = [KEY_PAST_ALL.0.to_be_bytes(), KEY_PAST_ALL.1.to_be_bytes()].concat();
    let past_key_dupsort = KEY_PAST_ALL.0.to_be_bytes().to_vec();

    assert!(
        simple_raw.set_range::<RawBytes<'_>, RawBytes<'_>>(&past_key_simple).unwrap().is_none()
    );
    assert!(
        dupsort_raw.set_range::<RawBytes<'_>, RawBytes<'_>>(&past_key_dupsort).unwrap().is_none()
    );

    // Both return last entry from prev() after set_range NOTFOUND.
    let simple_prev = simple_raw.prev::<RawBytes<'_>, RawBytes<'_>>().unwrap();
    let dupsort_prev = dupsort_raw.prev::<RawBytes<'_>, RawBytes<'_>>().unwrap();
    assert_eq!(decode_simple(&simple_prev.unwrap()).0, LAST_ENTRY.0);
    assert_eq!(decode_dupsort(&dupsort_prev.unwrap()).0, LAST_ENTRY.0);
}

/// first → prev(BOF) → next: cursors diverge.
#[test]
fn raw_mdbx_cursor_first_prev_bof_next() {
    let (reader, simple_table_id, common_prefix_table_id, _temp_dir) = setup_boundary_test_tables();
    let rtxn = reader.begin_ro_txn().unwrap();
    let simple_table = rtxn.open_table(&simple_table_id).unwrap();
    let common_prefix_table = rtxn.open_table(&common_prefix_table_id).unwrap();

    let mut simple_db_cursor = simple_table.cursor(&rtxn).unwrap();
    let mut dupsort_db_cursor = common_prefix_table.cursor(&rtxn).unwrap();
    let simple_raw = &mut simple_db_cursor.cursor;
    let dupsort_raw = &mut dupsort_db_cursor.cursor;

    simple_raw.first::<RawBytes<'_>, RawBytes<'_>>().unwrap();
    dupsort_raw.first::<RawBytes<'_>, RawBytes<'_>>().unwrap();

    assert!(simple_raw.prev::<RawBytes<'_>, RawBytes<'_>>().unwrap().is_none());
    assert!(dupsort_raw.prev::<RawBytes<'_>, RawBytes<'_>>().unwrap().is_none());

    // After NOTFOUND, non-DUP_SORT cursor stays at first entry; DUP_SORT enters BOF.
    let simple_current = simple_raw.get_current::<RawBytes<'_>, RawBytes<'_>>().unwrap();
    let dupsort_current = dupsort_raw.get_current::<RawBytes<'_>, RawBytes<'_>>().unwrap();
    assert!(simple_current.is_some(), "non-DUP_SORT: get_current valid after prev-NOTFOUND");
    assert!(dupsort_current.is_none(), "DUP_SORT: get_current returns None after prev-NOTFOUND");

    // next() diverges after prev-NOTFOUND at BOF.
    let simple_next = simple_raw.next::<RawBytes<'_>, RawBytes<'_>>().unwrap();
    let dupsort_next = dupsort_raw.next::<RawBytes<'_>, RawBytes<'_>>().unwrap();
    let simple_next_decoded = decode_simple(&simple_next.unwrap());
    let dupsort_next_decoded = decode_dupsort(&dupsort_next.unwrap());
    assert_ne!(
        simple_next_decoded.0, dupsort_next_decoded.0,
        "raw cursors diverge after prev-NOTFOUND → next"
    );
}

/// last → 2x next(EOF) → prev: same divergence as single next.
#[test]
fn raw_mdbx_cursor_last_double_next_eof_prev() {
    let (reader, simple_table_id, common_prefix_table_id, _temp_dir) = setup_boundary_test_tables();
    let rtxn = reader.begin_ro_txn().unwrap();
    let simple_table = rtxn.open_table(&simple_table_id).unwrap();
    let common_prefix_table = rtxn.open_table(&common_prefix_table_id).unwrap();

    let mut simple_db_cursor = simple_table.cursor(&rtxn).unwrap();
    let mut dupsort_db_cursor = common_prefix_table.cursor(&rtxn).unwrap();
    let simple_raw = &mut simple_db_cursor.cursor;
    let dupsort_raw = &mut dupsort_db_cursor.cursor;

    simple_raw.last::<RawBytes<'_>, RawBytes<'_>>().unwrap();
    dupsort_raw.last::<RawBytes<'_>, RawBytes<'_>>().unwrap();
    simple_raw.next::<RawBytes<'_>, RawBytes<'_>>().unwrap();
    dupsort_raw.next::<RawBytes<'_>, RawBytes<'_>>().unwrap();
    simple_raw.next::<RawBytes<'_>, RawBytes<'_>>().unwrap();
    dupsort_raw.next::<RawBytes<'_>, RawBytes<'_>>().unwrap();

    let simple_prev = simple_raw.prev::<RawBytes<'_>, RawBytes<'_>>().unwrap();
    let dupsort_prev = dupsort_raw.prev::<RawBytes<'_>, RawBytes<'_>>().unwrap();
    assert_eq!(
        decode_simple(&simple_prev.unwrap()).0,
        THIRD_ENTRY.0,
        "simple: double-next EOF then prev"
    );
    let dupsort_decoded = decode_dupsort(&dupsort_prev.unwrap());
    assert_ne!(
        THIRD_ENTRY.0, dupsort_decoded.0,
        "DUP_SORT raw cursor diverges from non-DUP_SORT after double-next EOF"
    );
}

/// Verifies that the compensated DbCursorTrait impl produces identical results for
/// SimpleTable (non-DUP_SORT) and CommonPrefix (DUP_SORT) across boundary scenarios.
///
/// The DUP_SORT DbCursorTrait impl re-seeks with last()/first() after NOTFOUND to
/// produce the same observable behavior as the SimpleTable (non-DUP_SORT) wrapper.
#[test]
fn compensated_dupsort_matches_simple() {
    let (reader, simple_table_id, common_prefix_table_id, _temp_dir) = setup_boundary_test_tables();

    let rtxn = reader.begin_ro_txn().unwrap();
    let simple_table = rtxn.open_table(&simple_table_id).unwrap();
    let common_prefix_table = rtxn.open_table(&common_prefix_table_id).unwrap();

    // lower_bound at various positions → next → prev.
    let test_keys: Vec<TableKey> = vec![
        KEY_BEFORE_ALL,
        GAP_WITHIN_MAIN_KEY,
        GAP_PAST_LAST_SUB_KEY,
        GAP_BETWEEN_MAIN_KEYS,
        KEY_PAST_ALL,
    ];
    for test_key in &test_keys {
        let mut simple_cursor = simple_table.cursor(&rtxn).unwrap();
        let mut common_prefix_cursor = common_prefix_table.cursor(&rtxn).unwrap();
        let simple_result = simple_cursor.lower_bound(test_key).unwrap();
        let common_prefix_result = common_prefix_cursor.lower_bound(test_key).unwrap();
        assert_eq!(simple_result, common_prefix_result, "compensated lower_bound({test_key:?})");

        let simple_next = simple_cursor.next().unwrap();
        let common_prefix_next = common_prefix_cursor.next().unwrap();
        assert_eq!(
            simple_next, common_prefix_next,
            "compensated next after lower_bound({test_key:?})"
        );

        let simple_prev = simple_cursor.prev().unwrap();
        let common_prefix_prev = common_prefix_cursor.prev().unwrap();
        assert_eq!(
            simple_prev, common_prefix_prev,
            "compensated prev after next after lower_bound({test_key:?})"
        );
    }

    // lower_bound past end → prev.
    {
        let mut simple_cursor = simple_table.cursor(&rtxn).unwrap();
        let mut common_prefix_cursor = common_prefix_table.cursor(&rtxn).unwrap();
        assert_eq!(simple_cursor.lower_bound(&KEY_PAST_ALL).unwrap(), None);
        assert_eq!(common_prefix_cursor.lower_bound(&KEY_PAST_ALL).unwrap(), None);
        let simple_prev = simple_cursor.prev().unwrap();
        let common_prefix_prev = common_prefix_cursor.prev().unwrap();
        assert_eq!(simple_prev, common_prefix_prev, "compensated: lower_bound past end → prev");
    }

    // Position at last → next → next → prev.
    {
        let mut simple_cursor = simple_table.cursor(&rtxn).unwrap();
        let mut common_prefix_cursor = common_prefix_table.cursor(&rtxn).unwrap();
        simple_cursor.lower_bound(&LAST_ENTRY.0).unwrap();
        common_prefix_cursor.lower_bound(&LAST_ENTRY.0).unwrap();
        simple_cursor.next().unwrap();
        common_prefix_cursor.next().unwrap();
        simple_cursor.next().unwrap();
        common_prefix_cursor.next().unwrap();
        let simple_prev = simple_cursor.prev().unwrap();
        let common_prefix_prev = common_prefix_cursor.prev().unwrap();
        assert_eq!(simple_prev, common_prefix_prev, "compensated: last → 2x next → prev");
    }

    // lower_bound past end → next → prev.
    {
        let mut simple_cursor = simple_table.cursor(&rtxn).unwrap();
        let mut common_prefix_cursor = common_prefix_table.cursor(&rtxn).unwrap();
        simple_cursor.lower_bound(&KEY_PAST_ALL).unwrap();
        common_prefix_cursor.lower_bound(&KEY_PAST_ALL).unwrap();
        simple_cursor.next().unwrap();
        common_prefix_cursor.next().unwrap();
        let simple_prev = simple_cursor.prev().unwrap();
        let common_prefix_prev = common_prefix_cursor.prev().unwrap();
        assert_eq!(
            simple_prev, common_prefix_prev,
            "compensated: lower_bound past end → next → prev"
        );
    }

    // first → prev → next.
    {
        let mut simple_cursor = simple_table.cursor(&rtxn).unwrap();
        let mut common_prefix_cursor = common_prefix_table.cursor(&rtxn).unwrap();
        simple_cursor.lower_bound(&FIRST_ENTRY.0).unwrap();
        common_prefix_cursor.lower_bound(&FIRST_ENTRY.0).unwrap();
        simple_cursor.prev().unwrap();
        common_prefix_cursor.prev().unwrap();
        let simple_next = simple_cursor.next().unwrap();
        let common_prefix_next = common_prefix_cursor.next().unwrap();
        assert_eq!(simple_next, common_prefix_next, "compensated: first → prev → next");
    }

    // first → prev → prev → next.
    {
        let mut simple_cursor = simple_table.cursor(&rtxn).unwrap();
        let mut common_prefix_cursor = common_prefix_table.cursor(&rtxn).unwrap();
        simple_cursor.lower_bound(&FIRST_ENTRY.0).unwrap();
        common_prefix_cursor.lower_bound(&FIRST_ENTRY.0).unwrap();
        simple_cursor.prev().unwrap();
        common_prefix_cursor.prev().unwrap();
        simple_cursor.prev().unwrap();
        common_prefix_cursor.prev().unwrap();
        let simple_next = simple_cursor.next().unwrap();
        let common_prefix_next = common_prefix_cursor.next().unwrap();
        assert_eq!(simple_next, common_prefix_next, "compensated: first → 2x prev → next");
    }

    // Full backward iteration from past-end.
    {
        let mut simple_cursor = simple_table.cursor(&rtxn).unwrap();
        let mut common_prefix_cursor = common_prefix_table.cursor(&rtxn).unwrap();
        assert_eq!(simple_cursor.lower_bound(&KEY_PAST_ALL).unwrap(), None);
        assert_eq!(common_prefix_cursor.lower_bound(&KEY_PAST_ALL).unwrap(), None);
        for expected in BOUNDARY_TEST_ENTRIES.iter().rev() {
            let simple_prev = simple_cursor.prev().unwrap();
            let common_prefix_prev = common_prefix_cursor.prev().unwrap();
            assert_eq!(simple_prev, common_prefix_prev, "compensated: backward iteration");
            assert_eq!(simple_prev, Some(*expected));
        }
        assert_eq!(simple_cursor.prev().unwrap(), None);
        assert_eq!(common_prefix_cursor.prev().unwrap(), None);
    }

    // Full forward iteration from before-start.
    {
        let mut simple_cursor = simple_table.cursor(&rtxn).unwrap();
        let mut common_prefix_cursor = common_prefix_table.cursor(&rtxn).unwrap();
        assert_eq!(
            simple_cursor.lower_bound(&KEY_BEFORE_ALL).unwrap(),
            Some(BOUNDARY_TEST_ENTRIES[0])
        );
        assert_eq!(
            common_prefix_cursor.lower_bound(&KEY_BEFORE_ALL).unwrap(),
            Some(BOUNDARY_TEST_ENTRIES[0])
        );
        for expected in BOUNDARY_TEST_ENTRIES.iter().skip(1) {
            let simple_next = simple_cursor.next().unwrap();
            let common_prefix_next = common_prefix_cursor.next().unwrap();
            assert_eq!(simple_next, common_prefix_next, "compensated: forward iteration");
            assert_eq!(simple_next, Some(*expected));
        }
        assert_eq!(simple_cursor.next().unwrap(), None);
        assert_eq!(common_prefix_cursor.next().unwrap(), None);
    }
}

// Ignore because this test takes few seconds to run.
#[ignore]
#[test]
fn common_prefix_compare_with_simple_table_random() {
    let ((reader, mut writer), _temp_dir) = get_test_env();
    let simple_table = writer.create_simple_table("simple_table").unwrap();
    let common_prefix_table = writer.create_common_prefix_table("common_prefix_table").unwrap();
    random_table_test(simple_table, common_prefix_table, &reader, &mut writer);
}

#[test]
fn common_prefix_append_greater_sub_key() {
    append_greater_sub_key_test(DbWriter::create_common_prefix_table);
}

#[allow(clippy::type_complexity)]
fn append_greater_sub_key_test<T>(
    create_table: fn(
        &mut DbWriter,
        &'static str,
    ) -> DbResult<TableIdentifier<TableKey, TableValue, T>>,
) where
    T: DupSortTableType + DupSortUtils<(u32, u32), NoVersionValueWrapper<u32>>,
{
    let ((_reader, mut writer), _temp_dir) = get_test_env();
    let table_id = create_table(&mut writer, "table").unwrap();

    let txn = writer.begin_rw_txn().unwrap();

    let handle = txn.open_table(&table_id).unwrap();
    handle.append_greater_sub_key(&txn, &(2, 2), &22).unwrap();
    handle.append_greater_sub_key(&txn, &(2, 3), &23).unwrap();
    handle.append_greater_sub_key(&txn, &(1, 1), &11).unwrap();
    handle.append_greater_sub_key(&txn, &(3, 0), &30).unwrap();

    // For DupSort tables append with key that already exists should fail. Try append with smaller
    // bigger and equal values.
    let result = handle.append_greater_sub_key(&txn, &(2, 2), &0);
    assert_matches!(result, Err(DbError::Append));

    let result = handle.append_greater_sub_key(&txn, &(2, 2), &22);
    assert_matches!(result, Err(DbError::Append));

    let result = handle.append_greater_sub_key(&txn, &(2, 2), &100);
    assert_matches!(result, Err(DbError::Append));

    // As before, but for the last main key.
    let result = handle.append_greater_sub_key(&txn, &(3, 0), &0);
    assert_matches!(result, Err(DbError::Append));

    let result = handle.append_greater_sub_key(&txn, &(3, 0), &30);
    assert_matches!(result, Err(DbError::Append));

    let result = handle.append_greater_sub_key(&txn, &(3, 0), &100);
    assert_matches!(result, Err(DbError::Append));

    // Check the final database.
    assert_eq!(handle.get(&txn, &(2, 2)).unwrap(), Some(22));
    assert_eq!(handle.get(&txn, &(2, 3)).unwrap(), Some(23));
    assert_eq!(handle.get(&txn, &(1, 1)).unwrap(), Some(11));
    assert_eq!(handle.get(&txn, &(3, 0)).unwrap(), Some(30));
}
