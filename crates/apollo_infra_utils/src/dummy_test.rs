use rstest::rstest;

#[rstest]
#[case(1)]
#[case(2)]
fn test_dur(#[case] dur: i32) {
    let _x = crate::register_magic_constants!(format!("{dur}"));
    assert!(dur < 3);
}
