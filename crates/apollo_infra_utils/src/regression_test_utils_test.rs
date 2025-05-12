use rstest::rstest;

use crate::register_magic_constants;

enum MagicPanicScenario {
    None,
    MissingKey,
    WrongValue,
}

#[rstest]
#[case::case_1_3(1, 3, MagicPanicScenario::None)]
#[case::case_2_3(2, 3, MagicPanicScenario::None)]
#[case::case_1_4(1, 4, MagicPanicScenario::None)]
#[case::case_2_4(2, 4, MagicPanicScenario::None)]
#[should_panic]
#[case::missing_key(1, 3, MagicPanicScenario::MissingKey)]
#[should_panic]
#[case::wrong_value(2, 3, MagicPanicScenario::WrongValue)]
fn test_magic_constants(
    #[case] a: u32,
    #[case] b: u32,
    #[case] panic_scenario: MagicPanicScenario,
) {
    let mut magic = register_magic_constants!();
    // Dependent keys.
    magic.assert_eq(&format!("7_PLUS_A_{a}"), 7 + a);
    magic.assert_eq(&format!("1_PLUS_B_{b}"), 1 + b);
    magic.assert_eq(&format!("A_{a}_PLUS_B_{b}"), a + b);
    // Independent keys.
    magic.assert_eq("C", 3);
    // Panic scenarios.
    match panic_scenario {
        MagicPanicScenario::None => {}
        MagicPanicScenario::MissingKey => {
            magic.assert_eq("NO_SUCH_KEY", 1);
        }
        MagicPanicScenario::WrongValue => {
            magic.assert_eq("7_PLUS_A_1", 100);
        }
    }
}
