use std::path::PathBuf;

use apollo_infra_utils::cairo0_compiler::cairo0_format;
use apollo_infra_utils::compile_time_cargo_manifest_dir;
use expect_test::expect_file;
use rstest::rstest;
use tokio::task::JoinSet;

use crate::CAIRO_FILES_MAP;

#[rstest]
#[tokio::test(flavor = "multi_thread")]
async fn test_cairo0_formatting() {
    // Get the path to the apollo_starknet_os directory.
    let apollo_starknet_os_path =
        PathBuf::from(compile_time_cargo_manifest_dir!()).join("src/cairo");

    let mut tasks = JoinSet::new();

    // Read all cairo files recursively from the directory.
    for (relative_path, contents) in CAIRO_FILES_MAP.iter() {
        let path = apollo_starknet_os_path.join(relative_path);
        tasks.spawn(async move {
            let formatted = cairo0_format(contents);
            expect_file![path].assert_eq(&formatted);
        });
    }

    tasks.join_all().await;
}
