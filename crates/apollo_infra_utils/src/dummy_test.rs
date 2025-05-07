use rstest::rstest;

#[rstest]
#[case(1)]
#[case(2)]
fn test_dur_foo(#[case] dur: i32) {
    let mut magic = crate::register_magic_constants!(format!("{dur}"));
    magic.assert_eq("TWICE", dur * 2);
    magic.assert_eq("THRICE", dur * 3);
}
