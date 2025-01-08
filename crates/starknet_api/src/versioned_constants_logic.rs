use std::io;

use thiserror::Error;

use crate::block::StarknetVersion;

#[derive(Debug, Error)]
pub enum VersionedConstantsError {
    #[error(transparent)]
    IoError(#[from] io::Error),
    #[error("JSON file cannot be serialized into VersionedConstants: {0}")]
    ParseError(#[from] serde_json::Error),
    #[error("Invalid version: {version:?}")]
    InvalidVersion { version: String },
    #[error("Invalid Starknet version: {0}")]
    InvalidStarknetVersion(StarknetVersion),
}

pub type VersionedConstantsResult<T> = Result<T, VersionedConstantsError>;

/// Auto-generate getters for listed versioned constants versions.
#[macro_export]
macro_rules! define_versioned_constants {
    ($struct_name:ident, $(($variant:ident, $path_to_json:expr)),* $(,)?) => {
        use starknet_infra_utils::compile_time_cargo_manifest_dir;
        use paste::paste;
        use std::fs;
        use std::path::PathBuf;
        use std::sync::LazyLock;
        use starknet_api::block::StarknetVersion;
        use starknet_api::versioned_constants_logic::{
            VersionedConstantsError, VersionedConstantsResult,
        };
        // Static (lazy) instances of the versioned constants.
        // For internal use only; for access to a static instance use the `StarknetVersion` enum.
        paste! {
            $(
                pub(crate) const [<VERSIONED_CONSTANTS_ $variant:upper _JSON>]: &str =
                    include_str!($path_to_json);
                /// Static instance of the versioned constants for the Starknet version.
                pub static [<VERSIONED_CONSTANTS_ $variant:upper>]: LazyLock<VersionedConstants> =
                    LazyLock::new(|| {
                        serde_json::from_str([<VERSIONED_CONSTANTS_ $variant:upper _JSON>])
                            .expect(&format!("Versioned constants {} is malformed.", $path_to_json))
                });
            )*
        }

        /// API to access a static instance of the versioned constants.
        impl TryFrom<StarknetVersion> for &'static $struct_name {
            type Error = VersionedConstantsError;

            fn try_from(version: StarknetVersion) -> VersionedConstantsResult<Self> {
                match version {
                    $(
                        StarknetVersion::$variant => {
                           Ok(& paste! { [<VERSIONED_CONSTANTS_ $variant:upper>] })
                        }
                    )*
                    _ => Err(VersionedConstantsError::InvalidStarknetVersion(version)),
                }
            }
        }

        impl $struct_name {
            /// Gets the path to the JSON file for the specified Starknet version.
            pub fn path_to_json(version: &StarknetVersion) -> VersionedConstantsResult<&'static str> {
                match version {
                    $(StarknetVersion::$variant => Ok($path_to_json),)*
                    _ => Err(VersionedConstantsError::InvalidStarknetVersion(*version)),
                }
            }

            /// Gets the constants that shipped with the current version of the Starknet.
            /// To use custom constants, initialize the struct from a file using `from_path`.
            pub fn latest_constants() -> &'static Self {
                Self::get(&StarknetVersion::LATEST)
                    .expect("Latest version should support VC.")
            }

            /// Gets the constants for the specified Starknet version.
            pub fn get(version: &StarknetVersion) -> VersionedConstantsResult<&'static Self> {
                match version {
                    $(
                        StarknetVersion::$variant => Ok(
                            & paste! { [<VERSIONED_CONSTANTS_ $variant:upper>] }
                        ),
                    )*
                    _ => Err(VersionedConstantsError::InvalidStarknetVersion(*version)),
                }
            }
        }

        /// Gets a string of the constants of the latest version of Starknet.
        pub static VERSIONED_CONSTANTS_LATEST_JSON: LazyLock<String> = LazyLock::new(|| {
            let latest_variant = StarknetVersion::LATEST;
            let path_to_json: PathBuf = [
                compile_time_cargo_manifest_dir!(),
                "src".into(),
                VersionedConstants::path_to_json(&latest_variant)
                    .expect("Latest variant should have a path to json.").into()
            ].iter().collect();
            fs::read_to_string(path_to_json.clone())
                .expect(&format!("Failed to read file {}.", path_to_json.display()))
        });
    };
}
