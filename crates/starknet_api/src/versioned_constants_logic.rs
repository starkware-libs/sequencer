use std::fmt::Debug;
#[cfg(any(test, feature = "testing"))]
use std::path::Path;

#[cfg(any(test, feature = "testing"))]
use expect_test::expect_file;
#[cfg(any(test, feature = "testing"))]
use json_patch::{
    diff as json_diff,
    AddOperation,
    PatchOperation,
    RemoveOperation,
    ReplaceOperation,
};
#[cfg(any(test, feature = "testing"))]
use serde_json::Value;
#[cfg(any(test, feature = "testing"))]
use strum::IntoEnumIterator;

use crate::block::StarknetVersion;

pub trait VersionedConstantsTrait: Debug {
    type Error: Debug;

    /// Gets the first version with versioned constants.
    fn first_version() -> StarknetVersion;

    /// Gets the contents of the JSON file for the specified Starknet version.
    fn json_str(version: &StarknetVersion) -> Result<&'static str, Self::Error>;

    /// Gets the constants that shipped with the current version of the Starknet.
    /// To use custom constants, initialize the struct from a file using `from_path`.
    fn latest_constants() -> &'static Self;

    /// Gets the constants for the specified Starknet version.
    fn get(version: &StarknetVersion) -> Result<&'static Self, Self::Error>;
}

/// Helper function to test for regression in diffs between versioned constant values.
/// An instance of this test is generated for each versioned constants struct generated via the
/// [crate::define_versioned_constants] macro.
#[cfg(any(test, feature = "testing"))]
pub fn vc_diffs_regression_test_body<V: VersionedConstantsTrait>(path_to_diff_dir: &Path) {
    let first_version = V::first_version();
    let mut prev_vc = serde_json::from_str::<Value>(V::json_str(&first_version).unwrap()).unwrap();
    let mut prev_version = first_version;
    for version in StarknetVersion::iter().filter(|v| v > &first_version) {
        let current_vc = serde_json::from_str::<Value>(V::json_str(&version).unwrap()).unwrap();
        let diff = json_diff(&prev_vc, &current_vc);
        let diff_string = diff
            .0
            .iter()
            .map(|patch| match patch {
                PatchOperation::Add(AddOperation { path, value }) => format!("+ {path}: {value}"),
                PatchOperation::Remove(RemoveOperation { path }) => format!("- {path}"),
                PatchOperation::Replace(ReplaceOperation { path, value }) => {
                    format!("~ {path}: {value}")
                }
                other => panic!("Unexpected patch operation: {other}"),
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        expect_file![path_to_diff_dir.join(format!("{prev_version}_{version}.txt"))]
            .assert_eq(&diff_string);
        prev_version = version;
        prev_vc = current_vc;
    }
}

/// Auto-generate getters for listed versioned constants versions.
/// Optionally provide an intermediate struct for deserialization.
/// Also, provide the path to the directory containing the diffs between versions (for initial
/// definition of the struct, run `UPDATE_EXPECT=1 cargo test -p <crate> test_vc_diffs_regression`
/// to generate the diffs).
#[macro_export]
macro_rules! define_versioned_constants {
    (
        $struct_name:ident,
        $error_type:ident,
        $first_version:expr_2021,
        $path_to_diff_dir:literal,
        $(($variant:ident, $path_to_json:expr_2021)),* $(,)?
    ) => {
        // Static (lazy) instances of the versioned constants.
        // For internal use only; for access to a static instance use the `StarknetVersion` enum.
        paste::paste! {
            $(
                /// Static instance of the versioned constants for the Starknet version.
                pub static [<VERSIONED_CONSTANTS_ $variant:upper>]: std::sync::LazyLock<$struct_name> =
                    std::sync::LazyLock::new(|| {
                        serde_json::from_str([<VERSIONED_CONSTANTS_ $variant:upper _JSON>])
                            .expect(&format!("Versioned constants {} is malformed.", $path_to_json))
                });
            )*
        }

        $crate::define_versioned_constants_inner!(
            $struct_name,
            $error_type,
            $first_version,
            $path_to_diff_dir,
            $(($variant, $path_to_json)),*
        );
    };

    (
        $struct_name:ident,
        $intermediate_struct_name:ident,
        $error_type:ident,
        $first_version:expr_2021,
        $path_to_diff_dir:literal,
        $(($variant:ident, $path_to_json:expr_2021)),* $(,)?
    ) => {
        // Static (lazy) instances of the versioned constants.
        // For internal use only; for access to a static instance use the `StarknetVersion` enum.
        paste::paste! {
            $(
                /// Static instance of the versioned constants for the Starknet version.
                pub static [<VERSIONED_CONSTANTS_ $variant:upper>]: std::sync::LazyLock<$struct_name> =
                    std::sync::LazyLock::new(|| {
                        serde_json::from_str::<$intermediate_struct_name>(
                            [<VERSIONED_CONSTANTS_ $variant:upper _JSON>]
                        )
                        .expect(&format!("Versioned constants {} is malformed.", $path_to_json))
                        .into()
                });
            )*
        }

        $crate::define_versioned_constants_inner!(
            $struct_name,
            $error_type,
            $first_version,
            $path_to_diff_dir,
            $(($variant, $path_to_json)),*
        );
    };
}

#[macro_export]
macro_rules! define_versioned_constants_inner {
    (
        $struct_name:ident,
        $error_type:ident,
        $first_version:expr_2021,
        $path_to_diff_dir:literal,
        $(($variant:ident, $path_to_json:expr_2021)),* $(,)?
    ) => {
        paste::paste! {
            $(
                pub(crate) const [<VERSIONED_CONSTANTS_ $variant:upper _JSON>]: &str =
                    include_str!($path_to_json);
            )*
        }

        impl starknet_api::versioned_constants_logic::VersionedConstantsTrait for $struct_name {
            type Error = $error_type;

            fn first_version() -> StarknetVersion {
                $first_version
            }

            fn json_str(
                version: &starknet_api::block::StarknetVersion
            ) -> Result<&'static str, Self::Error> {
                match version {
                    $(starknet_api::block::StarknetVersion::$variant => paste::paste! {
                        Ok([<VERSIONED_CONSTANTS_ $variant:upper _JSON>])
                    },)*
                    _ => Err(Self::Error::InvalidStarknetVersion(*version)),
                }
            }

            fn latest_constants() -> &'static Self {
                Self::get(&starknet_api::block::StarknetVersion::LATEST)
                    .expect("Latest version should support VC.")
            }

            fn get(
                version: &starknet_api::block::StarknetVersion
            ) -> Result<&'static Self, Self::Error> {
                match version {
                    $(
                        starknet_api::block::StarknetVersion::$variant => Ok(
                            & paste::paste! { [<VERSIONED_CONSTANTS_ $variant:upper>] }
                        ),
                    )*
                    _ => Err(Self::Error::InvalidStarknetVersion(*version)),
                }
            }
        }

        /// Gets a string of the constants of the latest version of Starknet.
        pub static VERSIONED_CONSTANTS_LATEST_JSON: std::sync::LazyLock<String> =
            std::sync::LazyLock::new(|| {
                let latest_variant = StarknetVersion::LATEST;
                $struct_name::json_str(&latest_variant)
                    .expect("Latest version should support VC.").to_string()
            });

        #[cfg(test)]
        #[test]
        fn test_vc_diffs_regression() {
            let path = std::path::Path::new($path_to_diff_dir);
            if !path.exists() {
                std::fs::create_dir_all(path).unwrap();
            }
            $crate::versioned_constants_logic::vc_diffs_regression_test_body::<$struct_name>(
                &path.canonicalize().unwrap()
            );
        }
    };
}
