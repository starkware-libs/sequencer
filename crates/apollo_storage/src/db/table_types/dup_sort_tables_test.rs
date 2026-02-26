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
    let simple_table_id = writer.create_simple_table::<TableKey, TableValue>("simple").unwrap();
    let common_prefix_table_id = writer.create_common_prefix_table::<u32, u32, TableValue>("cp").unwrap();

    let entries: Vec<(TableKey, u32)> =
        vec![((10, 20), 100), ((10, 50), 200), ((30, 10), 300), ((30, 40), 400)];

    let wtxn = writer.begin_rw_txn().unwrap();
    let simple_handle = wtxn.open_table(&simple_table_id).unwrap();
    let common_prefix_handle = wtxn.open_table(&common_prefix_table_id).unwrap();
    for &(key, val) in &entries {
        simple_handle.upsert(&wtxn, &key, &val).unwrap();
        common_prefix_handle.upsert(&wtxn, &key, &val).unwrap();
    }
    wtxn.commit().unwrap();

    let rtxn = reader.begin_ro_txn().unwrap();
    let simple_table = rtxn.open_table(&simple_table_id).unwrap();
    let common_prefix_table = rtxn.open_table(&common_prefix_table_id).unwrap();

    // Access raw cursors via the DbCursor struct's inner cursor field.
    let mut simple_db_cursor = simple_table.cursor(&rtxn).unwrap();
    let mut dupsort_db_cursor = common_prefix_table.cursor(&rtxn).unwrap();
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
    let simple_current = simple_raw.get_current::<Cow<'_, [u8]>, Cow<'_, [u8]>>().unwrap();
    let dupsort_current = dupsort_raw.get_current::<Cow<'_, [u8]>, Cow<'_, [u8]>>().unwrap();
    assert!(simple_current.is_some(), "non-DUP_SORT: get_current valid after NOTFOUND");
    assert!(dupsort_current.is_none(), "DUP_SORT: get_current returns None after NOTFOUND");

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
    let mut dupsort_db_cursor = common_prefix_table.cursor(&rtxn).unwrap();
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
    let mut dupsort_db_cursor = common_prefix_table.cursor(&rtxn).unwrap();
    let simple_raw = &mut simple_db_cursor.cursor;
    let dupsort_raw = &mut dupsort_db_cursor.cursor;

    simple_raw.first::<Cow<'_, [u8]>, Cow<'_, [u8]>>().unwrap();
    dupsort_raw.first::<Cow<'_, [u8]>, Cow<'_, [u8]>>().unwrap();

    assert!(simple_raw.prev::<Cow<'_, [u8]>, Cow<'_, [u8]>>().unwrap().is_none());
    assert!(dupsort_raw.prev::<Cow<'_, [u8]>, Cow<'_, [u8]>>().unwrap().is_none());

    // After NOTFOUND, non-DUP_SORT cursor stays at first entry; DUP_SORT enters BOF.
    let simple_current = simple_raw.get_current::<Cow<'_, [u8]>, Cow<'_, [u8]>>().unwrap();
    let dupsort_current = dupsort_raw.get_current::<Cow<'_, [u8]>, Cow<'_, [u8]>>().unwrap();
    assert!(simple_current.is_some(), "non-DUP_SORT: get_current valid after prev-NOTFOUND");
    assert!(dupsort_current.is_none(), "DUP_SORT: get_current returns None after prev-NOTFOUND");

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
    let mut dupsort_db_cursor = common_prefix_table.cursor(&rtxn).unwrap();
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
        let mut simple_cursor= simple_table.cursor(&rtxn).unwrap();
        let mut common_prefix_cursor= common_prefix_table.cursor(&rtxn).unwrap();
        let simple_result= simple_cursor.lower_bound(test_key).unwrap();
        let common_prefix_result= common_prefix_cursor.lower_bound(test_key).unwrap();
        assert_eq!(simple_result, common_prefix_result, "compensated lower_bound({test_key:?})");

        let simple_next= simple_cursor.next().unwrap();
        let common_prefix_next= common_prefix_cursor.next().unwrap();
        assert_eq!(simple_next, common_prefix_next, "compensated next after lower_bound({test_key:?})");

        let simple_prev= simple_cursor.prev().unwrap();
        let common_prefix_prev= common_prefix_cursor.prev().unwrap();
        assert_eq!(simple_prev, common_prefix_prev, "compensated prev after next after lower_bound({test_key:?})");
    }

    // lower_bound past end → prev.
    {
        let mut simple_cursor= simple_table.cursor(&rtxn).unwrap();
        let mut common_prefix_cursor= common_prefix_table.cursor(&rtxn).unwrap();
        assert_eq!(simple_cursor.lower_bound(&(40, 0)).unwrap(), None);
        assert_eq!(common_prefix_cursor.lower_bound(&(40, 0)).unwrap(), None);
        let simple_prev= simple_cursor.prev().unwrap();
        let common_prefix_prev= common_prefix_cursor.prev().unwrap();
        assert_eq!(simple_prev, common_prefix_prev, "compensated: lower_bound past end → prev");
    }

    // Position at last → next → next → prev.
    {
        let mut simple_cursor= simple_table.cursor(&rtxn).unwrap();
        let mut common_prefix_cursor= common_prefix_table.cursor(&rtxn).unwrap();
        simple_cursor.lower_bound(&(30, 40)).unwrap();
        common_prefix_cursor.lower_bound(&(30, 40)).unwrap();
        simple_cursor.next().unwrap();
        common_prefix_cursor.next().unwrap();
        simple_cursor.next().unwrap();
        common_prefix_cursor.next().unwrap();
        let simple_prev= simple_cursor.prev().unwrap();
        let common_prefix_prev= common_prefix_cursor.prev().unwrap();
        assert_eq!(simple_prev, common_prefix_prev, "compensated: last → 2x next → prev");
    }

    // lower_bound past end → next → prev.
    {
        let mut simple_cursor= simple_table.cursor(&rtxn).unwrap();
        let mut common_prefix_cursor= common_prefix_table.cursor(&rtxn).unwrap();
        simple_cursor.lower_bound(&(40, 0)).unwrap();
        common_prefix_cursor.lower_bound(&(40, 0)).unwrap();
        simple_cursor.next().unwrap();
        common_prefix_cursor.next().unwrap();
        let simple_prev= simple_cursor.prev().unwrap();
        let common_prefix_prev= common_prefix_cursor.prev().unwrap();
        assert_eq!(simple_prev, common_prefix_prev, "compensated: lower_bound past end → next → prev");
    }

    // first → prev → next.
    {
        let mut simple_cursor= simple_table.cursor(&rtxn).unwrap();
        let mut common_prefix_cursor= common_prefix_table.cursor(&rtxn).unwrap();
        simple_cursor.lower_bound(&(10, 20)).unwrap();
        common_prefix_cursor.lower_bound(&(10, 20)).unwrap();
        simple_cursor.prev().unwrap();
        common_prefix_cursor.prev().unwrap();
        let simple_next= simple_cursor.next().unwrap();
        let common_prefix_next= common_prefix_cursor.next().unwrap();
        assert_eq!(simple_next, common_prefix_next, "compensated: first → prev → next");
    }

    // first → prev → prev → next.
    {
        let mut simple_cursor= simple_table.cursor(&rtxn).unwrap();
        let mut common_prefix_cursor= common_prefix_table.cursor(&rtxn).unwrap();
        simple_cursor.lower_bound(&(10, 20)).unwrap();
        common_prefix_cursor.lower_bound(&(10, 20)).unwrap();
        simple_cursor.prev().unwrap();
        common_prefix_cursor.prev().unwrap();
        simple_cursor.prev().unwrap();
        common_prefix_cursor.prev().unwrap();
        let simple_next= simple_cursor.next().unwrap();
        let common_prefix_next= common_prefix_cursor.next().unwrap();
        assert_eq!(simple_next, common_prefix_next, "compensated: first → 2x prev → next");
    }

    // Full backward iteration from past-end.
    {
        let mut simple_cursor= simple_table.cursor(&rtxn).unwrap();
        let mut common_prefix_cursor= common_prefix_table.cursor(&rtxn).unwrap();
        assert_eq!(simple_cursor.lower_bound(&(40, 0)).unwrap(), None);
        assert_eq!(common_prefix_cursor.lower_bound(&(40, 0)).unwrap(), None);
        for expected in entries.iter().rev() {
            let simple_prev= simple_cursor.prev().unwrap();
            let common_prefix_prev= common_prefix_cursor.prev().unwrap();
            assert_eq!(simple_prev, common_prefix_prev, "compensated: backward iteration");
            assert_eq!(simple_prev,Some(*expected));
        }
        assert_eq!(simple_cursor.prev().unwrap(), None);
        assert_eq!(common_prefix_cursor.prev().unwrap(), None);
    }

    // Full forward iteration from before-start.
    {
        let mut simple_cursor= simple_table.cursor(&rtxn).unwrap();
        let mut common_prefix_cursor= common_prefix_table.cursor(&rtxn).unwrap();
        assert_eq!(simple_cursor.lower_bound(&(0, 0)).unwrap(), Some(entries[0]));
        assert_eq!(common_prefix_cursor.lower_bound(&(0, 0)).unwrap(), Some(entries[0]));
        for expected in entries.iter().skip(1) {
            let simple_next= simple_cursor.next().unwrap();
            let common_prefix_next= common_prefix_cursor.next().unwrap();
            assert_eq!(simple_next, common_prefix_next, "compensated: forward iteration");
            assert_eq!(simple_next,Some(*expected));
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
