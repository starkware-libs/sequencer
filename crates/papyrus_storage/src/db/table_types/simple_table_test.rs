use crate::db::DbWriter;
use crate::db::table_types::test_utils::table_test;

#[test]
fn simple_table_test() {
    table_test(DbWriter::create_simple_table);
}
