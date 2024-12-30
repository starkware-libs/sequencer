#[cfg(feature = "cairo_native")]
use cargo_metadata::{CargoOpt, MetadataCommand};

#[cfg(feature = "cairo_native")]
fn main() {
    let cairo_native_version = find_cairo_native_version();
    println!("cargo:rustc-env=DEP_CAIRO_NATIVE_VERSION={}", cairo_native_version);
}

#[cfg(feature = "cairo_native")]
fn find_cairo_native_version() -> String {
    let target_package_name = "cairo-native";

    // Query Cargo for the actual resolved metadata
    let metadata = MetadataCommand::new()
        .features(CargoOpt::SomeFeatures(vec!["cairo_native".into()]))
        .exec()
        .unwrap_or_else(|err| {
            eprintln!("Failed to read Cargo metadata: {}", err);
            std::process::exit(1);
        });

    // Find the package in the resolved graph
    let package = metadata.packages.iter().find(|package| package.name == target_package_name);

    package.expect("Could not find cairo-native package in Cargo metadata").version.to_string()
}

#[cfg(not(feature = "cairo_native"))]
fn main() {}
