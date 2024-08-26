/// Get the crate's `OUT_DIR` and navigates up to reach the `target/BUILD_FLAVOR` directory.
/// This directory is shared accross all crates in this project.
pub fn get_traget_build_flavor_dir() -> &'static std::path::Path {
    let out_dir = std::env::var("OUT_DIR").expect("Failed to get the OUT_DIR environment variable");
    // Navigate from this crate's build folder to reach the `target/BUILD_FLAVOR` directory.
    Box::leak(
        std::path::Path::new(&out_dir)
            .ancestors()
            .nth(3)
            .expect("Failed to navigate up three levels from OUT_DIR")
            .into(),
    )
}
