use expect_test::expect_file;

const FUZZ_REVERT_CONTENTS: &str =
    include_str!("../resources/feature_contracts/cairo1/fuzz_revert.cairo");
const FUZZ_REVERT2_EXTRA_FUNCTION: &str = r#"
    /// This function is here to make this contract's class hash different from the main fuzz
    /// revert contract.
    #[external(v0)]
    fn dummy_function(ref self: ContractState) -> felt252 {
        return 100;
    }
"#;

#[test]
fn test_fuzz_revert_2_almost_identical() {
    let contents = FUZZ_REVERT_CONTENTS.to_string();
    let mut contents = contents.trim().lines().collect::<Vec<&str>>();
    contents.insert(0, "// This contract is auto-generated. To regenerate, run:");
    contents.insert(
        1,
        "// `UPDATE_EXPECT=1 cargo test -p blockifier_test_utils \
         test_fuzz_revert_2_almost_identical`",
    );
    let closing_brace = contents.pop().unwrap();
    contents.extend(FUZZ_REVERT2_EXTRA_FUNCTION.lines());
    contents.push(closing_brace);
    contents.push("");
    expect_file!["../resources/feature_contracts/cairo1/fuzz_revert_2.cairo"]
        .assert_eq(&contents.join("\n"));
}
