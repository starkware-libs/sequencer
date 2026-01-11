use expect_test::expect_file;
use strum::IntoEnumIterator;

use crate::presets::types::Preset;

#[test]
fn test_preset_regression() {
    for preset in Preset::iter() {
        let preset_fields = preset.preset_fields();
        expect_file![format!("./regression/{}.txt", preset.name())].assert_debug_eq(&preset_fields);
    }
}
