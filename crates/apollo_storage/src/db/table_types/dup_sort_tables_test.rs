use assert_matches::assert_matches;

use super::{DupSortTableType, DupSortUtils};
use crate::db::db_test::get_test_env;
use crate::db::serialization::NoVersionValueWrapper;
use crate::db::table_types::test_utils::{random_table_test, table_test, TableKey, TableValue};
use crate::db::table_types::{DbCursorTrait, Table};
use crate::db::{DbError, DbResult, DbWriter, TableIdentifier};

#[test]
fn common_prefix_table() {
    table_test(DbWriter::create_common_prefix_table);
}

/// Targeted test: insert a few entries, then verify lower_bound + next/prev consistency.
#[test]
fn common_prefix_lower_bound_cursor_state() {
    let ((reader, mut writer), _temp_dir) = get_test_env();
    let simple_id = writer.create_simple_table::<TableKey, TableValue>("simple").unwrap();
    let cp_id = writer.create_common_prefix_table::<u32, u32, TableValue>("cp").unwrap();

    // Insert entries into both tables.
    let entries: Vec<(TableKey, u32)> =
        vec![((10, 20), 100), ((10, 50), 200), ((30, 10), 300), ((30, 40), 400)];

    let wtxn = writer.begin_rw_txn().unwrap();
    let simple_handle = wtxn.open_table(&simple_id).unwrap();
    let cp_handle = wtxn.open_table(&cp_id).unwrap();
    for &(key, val) in &entries {
        simple_handle.upsert(&wtxn, &key, &val).unwrap();
        cp_handle.upsert(&wtxn, &key, &val).unwrap();
    }
    wtxn.commit().unwrap();

    let rtxn = reader.begin_ro_txn().unwrap();
    let simple_table = rtxn.open_table(&simple_id).unwrap();
    let cp_table = rtxn.open_table(&cp_id).unwrap();

    // Test: lower_bound on various keys, verify results match between simple and cp tables.
    let test_keys: Vec<TableKey> = vec![
        (10, 30), // Between two entries with same main_key
        (10, 60), // Past all entries for main_key 10 → should find (30, 10)
        (20, 0),  // main_key doesn't exist → should find (30, 10)
        (40, 0),  // Past all entries → should find None
        (0, 0),   // Before all entries → should find (10, 20)
    ];

    for test_key in &test_keys {
        let mut simple_cursor = simple_table.cursor(&rtxn).unwrap();
        let mut cp_cursor = cp_table.cursor(&rtxn).unwrap();
        let simple_result = simple_cursor.lower_bound(test_key).unwrap();
        let cp_result = cp_cursor.lower_bound(test_key).unwrap();
        assert_eq!(simple_result, cp_result, "lower_bound({test_key:?}) mismatch");

        // After lower_bound, verify next() matches.
        let simple_next = simple_cursor.next().unwrap();
        let cp_next = cp_cursor.next().unwrap();
        assert_eq!(simple_next, cp_next, "next after lower_bound({test_key:?}) mismatch");

        // And prev() matches.
        let simple_prev = simple_cursor.prev().unwrap();
        let cp_prev = cp_cursor.prev().unwrap();
        assert_eq!(
            simple_prev, cp_prev,
            "prev after next after lower_bound({test_key:?}) mismatch"
        );
    }

    // Test: lower_bound past all entries returns None, then prev() returns last entry.
    {
        let past_end_key = (40u32, 0u32);
        let mut simple_cursor = simple_table.cursor(&rtxn).unwrap();
        let mut cp_cursor = cp_table.cursor(&rtxn).unwrap();

        let simple_lb = simple_cursor.lower_bound(&past_end_key).unwrap();
        let cp_lb = cp_cursor.lower_bound(&past_end_key).unwrap();
        assert_eq!(simple_lb, None);
        assert_eq!(cp_lb, None);

        let simple_prev = simple_cursor.prev().unwrap();
        let cp_prev = cp_cursor.prev().unwrap();
        assert_eq!(simple_prev, cp_prev, "prev after lower_bound past end mismatch");
    }

    // Test: navigate to last entry, call next() to reach EOF, then prev().
    {
        let last_key = (30u32, 40u32);
        let mut simple_cursor = simple_table.cursor(&rtxn).unwrap();
        let mut cp_cursor = cp_table.cursor(&rtxn).unwrap();

        // Position at last entry.
        let simple_lb = simple_cursor.lower_bound(&last_key).unwrap();
        let cp_lb = cp_cursor.lower_bound(&last_key).unwrap();
        assert_eq!(simple_lb, cp_lb, "lower_bound at last key mismatch");

        // Go past the end.
        let simple_next = simple_cursor.next().unwrap();
        let cp_next = cp_cursor.next().unwrap();
        assert_eq!(simple_next, None);
        assert_eq!(cp_next, None);

        // Call next again to stay at EOF.
        let simple_next2 = simple_cursor.next().unwrap();
        let cp_next2 = cp_cursor.next().unwrap();
        assert_eq!(simple_next2, None);
        assert_eq!(cp_next2, None);

        // Now call prev — should return last entry.
        let simple_prev = simple_cursor.prev().unwrap();
        let cp_prev = cp_cursor.prev().unwrap();
        assert_eq!(simple_prev, cp_prev, "prev from EOF mismatch");
    }
}

/// Probes raw MDBX cursor behavior at table boundaries for DUP_SORT vs non-DUP_SORT.
///
/// After MDBX_NEXT or MDBX_PREV returns NOTFOUND, the cursor enters an EOF/unpositioned
/// state. Per MDBX semantics, relative operations from this state are undefined — you
/// must re-seek with an absolute operation before resuming relative navigation.
///
/// Non-DUP_SORT tables happen to leave get_current() valid after NOTFOUND (implementation
/// detail, not a contract), so the SimpleTable wrapper works without explicit re-seeking.
/// DUP_SORT tables properly enter EOF state (get_current returns None).
///
/// Entries (sorted): (10,20)→100, (10,50)→200, (30,10)→300, (30,40)→400
///
/// Verified scenarios (raw cursor, no compensation):
///   A) last → next(EOF) → prev: non-DUP returns (30,10), DUP returns (10,50)
///   B) set_range past end → prev: both return (30,40)
///   C) first → prev(BOF) → next: non-DUP returns (10,50), DUP returns (30,10)
///   D) last → 2x next(EOF) → prev: same as A
///
/// The DUP_SORT DbCursorTrait impl re-seeks with last()/first() after NOTFOUND to
/// produce the same observable behavior as the SimpleTable (non-DUP_SORT) wrapper.
#[test]
fn raw_mdbx_cursor_boundary_behavior() {
    use std::borrow::Cow;

    use byteorder::{BigEndian, ReadBytesExt};

    type RawKV<'a> = (Cow<'a, [u8]>, Cow<'a, [u8]>);

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

    let ((reader, mut writer), _temp_dir) = get_test_env();
    let simple_id = writer.create_simple_table::<TableKey, TableValue>("simple").unwrap();
    let cp_id = writer.create_common_prefix_table::<u32, u32, TableValue>("cp").unwrap();

    let entries: Vec<(TableKey, u32)> =
        vec![((10, 20), 100), ((10, 50), 200), ((30, 10), 300), ((30, 40), 400)];

    let wtxn = writer.begin_rw_txn().unwrap();
    let simple_handle = wtxn.open_table(&simple_id).unwrap();
    let cp_handle = wtxn.open_table(&cp_id).unwrap();
    for &(key, val) in &entries {
        simple_handle.upsert(&wtxn, &key, &val).unwrap();
        cp_handle.upsert(&wtxn, &key, &val).unwrap();
    }
    wtxn.commit().unwrap();

    let rtxn = reader.begin_ro_txn().unwrap();
    let simple_table = rtxn.open_table(&simple_id).unwrap();
    let cp_table = rtxn.open_table(&cp_id).unwrap();

    // Access raw cursors via the DbCursor struct's inner cursor field.
    let mut simple_db_cursor = simple_table.cursor(&rtxn).unwrap();
    let mut dupsort_db_cursor = cp_table.cursor(&rtxn).unwrap();
    let simple_raw = &mut simple_db_cursor.cursor;
    let dupsort_raw = &mut dupsort_db_cursor.cursor;

    // Scenario A: Position at last entry → next() → get_current → prev().

    let simple_last = simple_raw.last::<Cow<'_, [u8]>, Cow<'_, [u8]>>().unwrap().unwrap();
    let dupsort_last = dupsort_raw.last::<Cow<'_, [u8]>, Cow<'_, [u8]>>().unwrap().unwrap();
    assert_eq!(decode_simple(&simple_last).0, (30, 40));
    assert_eq!(decode_dupsort(&dupsort_last).0, (30, 40));

    let simple_next = simple_raw.next::<Cow<'_, [u8]>, Cow<'_, [u8]>>().unwrap();
    let dupsort_next = dupsort_raw.next::<Cow<'_, [u8]>, Cow<'_, [u8]>>().unwrap();
    assert!(simple_next.is_none());
    assert!(dupsort_next.is_none());

    // After NOTFOUND, non-DUP_SORT cursor stays at last entry; DUP_SORT enters EOF.
    let simple_cur = simple_raw.get_current::<Cow<'_, [u8]>, Cow<'_, [u8]>>().unwrap();
    let dupsort_cur = dupsort_raw.get_current::<Cow<'_, [u8]>, Cow<'_, [u8]>>().unwrap();
    assert!(simple_cur.is_some(), "non-DUP_SORT: get_current valid after NOTFOUND");
    assert!(dupsort_cur.is_none(), "DUP_SORT: get_current returns None after NOTFOUND");

    // prev() diverges: non-DUP_SORT returns second-to-last; DUP_SORT returns a different entry.
    let simple_prev = simple_raw.prev::<Cow<'_, [u8]>, Cow<'_, [u8]>>().unwrap();
    let dupsort_prev = dupsort_raw.prev::<Cow<'_, [u8]>, Cow<'_, [u8]>>().unwrap();
    let simple_prev_decoded = decode_simple(&simple_prev.unwrap());
    assert_eq!(
        simple_prev_decoded.0,
        (30, 10),
        "simple: prev from EOF should return second-to-last"
    );
    let dupsort_prev_decoded = decode_dupsort(&dupsort_prev.unwrap());
    assert_ne!(
        simple_prev_decoded.0, dupsort_prev_decoded.0,
        "raw cursors diverge after next-NOTFOUND → prev"
    );

    // Scenario B: set_range past all entries → prev().
    let mut simple_db_cursor = simple_table.cursor(&rtxn).unwrap();
    let mut dupsort_db_cursor = cp_table.cursor(&rtxn).unwrap();
    let simple_raw = &mut simple_db_cursor.cursor;
    let dupsort_raw = &mut dupsort_db_cursor.cursor;

    let mut past_key_simple = Vec::new();
    byteorder::WriteBytesExt::write_u32::<BigEndian>(&mut past_key_simple, 40).unwrap();
    byteorder::WriteBytesExt::write_u32::<BigEndian>(&mut past_key_simple, 0).unwrap();
    let mut past_key_dupsort = Vec::new();
    byteorder::WriteBytesExt::write_u32::<BigEndian>(&mut past_key_dupsort, 40).unwrap();

    assert!(
        simple_raw.set_range::<Cow<'_, [u8]>, Cow<'_, [u8]>>(&past_key_simple).unwrap().is_none()
    );
    assert!(
        dupsort_raw.set_range::<Cow<'_, [u8]>, Cow<'_, [u8]>>(&past_key_dupsort).unwrap().is_none()
    );

    // Both return last entry from prev() after set_range NOTFOUND.
    let simple_prev = simple_raw.prev::<Cow<'_, [u8]>, Cow<'_, [u8]>>().unwrap();
    let dupsort_prev = dupsort_raw.prev::<Cow<'_, [u8]>, Cow<'_, [u8]>>().unwrap();
    assert_eq!(decode_simple(&simple_prev.unwrap()).0, (30, 40));
    assert_eq!(decode_dupsort(&dupsort_prev.unwrap()).0, (30, 40));

    // Scenario C: Position at first → prev(NOTFOUND) → next().
    let mut simple_db_cursor = simple_table.cursor(&rtxn).unwrap();
    let mut dupsort_db_cursor = cp_table.cursor(&rtxn).unwrap();
    let simple_raw = &mut simple_db_cursor.cursor;
    let dupsort_raw = &mut dupsort_db_cursor.cursor;

    simple_raw.first::<Cow<'_, [u8]>, Cow<'_, [u8]>>().unwrap();
    dupsort_raw.first::<Cow<'_, [u8]>, Cow<'_, [u8]>>().unwrap();

    assert!(simple_raw.prev::<Cow<'_, [u8]>, Cow<'_, [u8]>>().unwrap().is_none());
    assert!(dupsort_raw.prev::<Cow<'_, [u8]>, Cow<'_, [u8]>>().unwrap().is_none());

    // next() diverges after prev-NOTFOUND at BOF.
    let simple_next = simple_raw.next::<Cow<'_, [u8]>, Cow<'_, [u8]>>().unwrap();
    let dupsort_next = dupsort_raw.next::<Cow<'_, [u8]>, Cow<'_, [u8]>>().unwrap();
    let simple_next_decoded = decode_simple(&simple_next.unwrap());
    let dupsort_next_decoded = decode_dupsort(&dupsort_next.unwrap());
    assert_ne!(
        simple_next_decoded.0, dupsort_next_decoded.0,
        "raw cursors diverge after prev-NOTFOUND → next"
    );

    // Scenario D: last → 2x next(NOTFOUND) → prev.
    let mut simple_db_cursor = simple_table.cursor(&rtxn).unwrap();
    let mut dupsort_db_cursor = cp_table.cursor(&rtxn).unwrap();
    let simple_raw = &mut simple_db_cursor.cursor;
    let dupsort_raw = &mut dupsort_db_cursor.cursor;

    simple_raw.last::<Cow<'_, [u8]>, Cow<'_, [u8]>>().unwrap();
    dupsort_raw.last::<Cow<'_, [u8]>, Cow<'_, [u8]>>().unwrap();
    simple_raw.next::<Cow<'_, [u8]>, Cow<'_, [u8]>>().unwrap();
    dupsort_raw.next::<Cow<'_, [u8]>, Cow<'_, [u8]>>().unwrap();
    simple_raw.next::<Cow<'_, [u8]>, Cow<'_, [u8]>>().unwrap();
    dupsort_raw.next::<Cow<'_, [u8]>, Cow<'_, [u8]>>().unwrap();

    let simple_prev = simple_raw.prev::<Cow<'_, [u8]>, Cow<'_, [u8]>>().unwrap();
    let dupsort_prev = dupsort_raw.prev::<Cow<'_, [u8]>, Cow<'_, [u8]>>().unwrap();
    assert_eq!(
        decode_simple(&simple_prev.unwrap()).0,
        (30, 10),
        "simple: double-next EOF then prev"
    );
    let dupsort_decoded = decode_dupsort(&dupsort_prev.unwrap());
    assert_ne!(
        (30u32, 10u32),
        dupsort_decoded.0,
        "DUP_SORT raw cursor diverges from non-DUP_SORT after double-next EOF"
    );

    // Verify compensated DbCursorTrait produces identical results for both table types.
    let test_keys: Vec<TableKey> = vec![(10, 30), (10, 60), (20, 0), (40, 0), (0, 0)];
    for test_key in &test_keys {
        let mut sc = simple_table.cursor(&rtxn).unwrap();
        let mut cc = cp_table.cursor(&rtxn).unwrap();
        let sr = sc.lower_bound(test_key).unwrap();
        let cr = cc.lower_bound(test_key).unwrap();
        assert_eq!(sr, cr, "compensated lower_bound({test_key:?})");

        let sn = sc.next().unwrap();
        let cn = cc.next().unwrap();
        assert_eq!(sn, cn, "compensated next after lower_bound({test_key:?})");

        let sp = sc.prev().unwrap();
        let cp = cc.prev().unwrap();
        assert_eq!(sp, cp, "compensated prev after next after lower_bound({test_key:?})");
    }

    // lower_bound past end → prev.
    {
        let mut sc = simple_table.cursor(&rtxn).unwrap();
        let mut cc = cp_table.cursor(&rtxn).unwrap();
        assert_eq!(sc.lower_bound(&(40, 0)).unwrap(), None);
        assert_eq!(cc.lower_bound(&(40, 0)).unwrap(), None);
        let sp = sc.prev().unwrap();
        let cp = cc.prev().unwrap();
        assert_eq!(sp, cp, "compensated: lower_bound past end → prev");
    }

    // Position at last → next → next → prev.
    {
        let mut sc = simple_table.cursor(&rtxn).unwrap();
        let mut cc = cp_table.cursor(&rtxn).unwrap();
        sc.lower_bound(&(30, 40)).unwrap();
        cc.lower_bound(&(30, 40)).unwrap();
        sc.next().unwrap();
        cc.next().unwrap();
        sc.next().unwrap();
        cc.next().unwrap();
        let sp = sc.prev().unwrap();
        let cp = cc.prev().unwrap();
        assert_eq!(sp, cp, "compensated: last → 2x next → prev");
    }

    // lower_bound past end → next → prev.
    {
        let mut sc = simple_table.cursor(&rtxn).unwrap();
        let mut cc = cp_table.cursor(&rtxn).unwrap();
        sc.lower_bound(&(40, 0)).unwrap();
        cc.lower_bound(&(40, 0)).unwrap();
        sc.next().unwrap();
        cc.next().unwrap();
        let sp = sc.prev().unwrap();
        let cp = cc.prev().unwrap();
        assert_eq!(sp, cp, "compensated: lower_bound past end → next → prev");
    }

    // first → prev → next.
    {
        let mut sc = simple_table.cursor(&rtxn).unwrap();
        let mut cc = cp_table.cursor(&rtxn).unwrap();
        sc.lower_bound(&(10, 20)).unwrap();
        cc.lower_bound(&(10, 20)).unwrap();
        sc.prev().unwrap();
        cc.prev().unwrap();
        let sn = sc.next().unwrap();
        let cn = cc.next().unwrap();
        assert_eq!(sn, cn, "compensated: first → prev → next");
    }

    // first → prev → prev → next.
    {
        let mut sc = simple_table.cursor(&rtxn).unwrap();
        let mut cc = cp_table.cursor(&rtxn).unwrap();
        sc.lower_bound(&(10, 20)).unwrap();
        cc.lower_bound(&(10, 20)).unwrap();
        sc.prev().unwrap();
        cc.prev().unwrap();
        sc.prev().unwrap();
        cc.prev().unwrap();
        let sn = sc.next().unwrap();
        let cn = cc.next().unwrap();
        assert_eq!(sn, cn, "compensated: first → 2x prev → next");
    }

    // Full backward iteration from past-end.
    {
        let mut sc = simple_table.cursor(&rtxn).unwrap();
        let mut cc = cp_table.cursor(&rtxn).unwrap();
        assert_eq!(sc.lower_bound(&(40, 0)).unwrap(), None);
        assert_eq!(cc.lower_bound(&(40, 0)).unwrap(), None);
        for expected in entries.iter().rev() {
            let sp = sc.prev().unwrap();
            let cp = cc.prev().unwrap();
            assert_eq!(sp, cp, "compensated: backward iteration");
            assert_eq!(sp, Some(*expected));
        }
        assert_eq!(sc.prev().unwrap(), None);
        assert_eq!(cc.prev().unwrap(), None);
    }

    // Full forward iteration from before-start.
    {
        let mut sc = simple_table.cursor(&rtxn).unwrap();
        let mut cc = cp_table.cursor(&rtxn).unwrap();
        assert_eq!(sc.lower_bound(&(0, 0)).unwrap(), Some(entries[0]));
        assert_eq!(cc.lower_bound(&(0, 0)).unwrap(), Some(entries[0]));
        for expected in entries.iter().skip(1) {
            let sn = sc.next().unwrap();
            let cn = cc.next().unwrap();
            assert_eq!(sn, cn, "compensated: forward iteration");
            assert_eq!(sn, Some(*expected));
        }
        assert_eq!(sc.next().unwrap(), None);
        assert_eq!(cc.next().unwrap(), None);
    }
}

/// Verify raw set_lowerbound NOTFOUND → prev() behavior for DUP_SORT.
/// Does it match set_range NOTFOUND → prev() (Scenario B)?
#[test]
fn raw_set_lowerbound_notfound_then_prev() {
    use std::borrow::Cow;

    use byteorder::{BigEndian, ReadBytesExt};

    let ((reader, mut writer), _temp_dir) = get_test_env();
    let cp_id = writer.create_common_prefix_table::<u32, u32, TableValue>("cp").unwrap();

    let entries: Vec<(TableKey, u32)> =
        vec![((10, 20), 100), ((10, 50), 200), ((30, 10), 300), ((30, 40), 400)];

    let wtxn = writer.begin_rw_txn().unwrap();
    let cp_handle = wtxn.open_table(&cp_id).unwrap();
    for &(key, val) in &entries {
        cp_handle.upsert(&wtxn, &key, &val).unwrap();
    }
    wtxn.commit().unwrap();

    let rtxn = reader.begin_ro_txn().unwrap();
    let cp_table = rtxn.open_table(&cp_id).unwrap();
    let mut db_cursor = cp_table.cursor(&rtxn).unwrap();
    let raw = &mut db_cursor.cursor;

    // set_lowerbound past all entries.
    let mut main_key = Vec::new();
    byteorder::WriteBytesExt::write_u32::<BigEndian>(&mut main_key, 40).unwrap();
    let mut sub_key = Vec::new();
    byteorder::WriteBytesExt::write_u32::<BigEndian>(&mut sub_key, 0).unwrap();

    let result =
        raw.set_lowerbound::<Cow<'_, [u8]>, Cow<'_, [u8]>>(&main_key, Some(&sub_key)).unwrap();
    assert!(result.is_none(), "set_lowerbound past end should return None");

    // prev() should return last entry.
    let prev_result = raw.prev::<Cow<'_, [u8]>, Cow<'_, [u8]>>().unwrap();
    let kv = prev_result.expect("set_lowerbound NOTFOUND → prev() should not be None");
    let mut k = kv.0.as_ref();
    let mk = k.read_u32::<BigEndian>().unwrap();
    let mut d = kv.1.as_ref();
    let sk = d.read_u32::<BigEndian>().unwrap();
    assert_eq!((mk, sk), (30, 40), "should return last entry");

    // Also test: set_lowerbound NOTFOUND → next() should return None.
    let mut db_cursor2 = cp_table.cursor(&rtxn).unwrap();
    let raw2 = &mut db_cursor2.cursor;
    let result2 =
        raw2.set_lowerbound::<Cow<'_, [u8]>, Cow<'_, [u8]>>(&main_key, Some(&sub_key)).unwrap();
    assert!(result2.is_none());
    let next_result = raw2.next::<Cow<'_, [u8]>, Cow<'_, [u8]>>().unwrap();
    if let Some(ref kv) = next_result {
        let mut k = kv.0.as_ref();
        let mk = k.read_u32::<BigEndian>().unwrap();
        let mut d = kv.1.as_ref();
        let sk = d.read_u32::<BigEndian>().unwrap();
        eprintln!("set_lowerbound NOTFOUND → next() = ({mk}, {sk}) [expected None]");
    }
    assert!(next_result.is_none(), "set_lowerbound NOTFOUND → next() should be None");
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
