use crate::block::StarknetVersion;

pub trait VersionedConstantsTrait {
    type Error;

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

/// Auto-generate getters for listed versioned constants versions.
/// Optionally provide an intermediate struct for deserialization.
#[macro_export]
macro_rules! define_versioned_constants {
    (
        $struct_name:ident,
        $error_type:ident,
        $first_version:expr,
        $(($variant:ident, $path_to_json:expr)),* $(,)?
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
            $struct_name, $error_type, $first_version, $(($variant, $path_to_json)),*
        );
    };

    (
        $struct_name:ident,
        $intermediate_struct_name:ident,
        $error_type:ident,
        $first_version:expr,
        $(($variant:ident, $path_to_json:expr)),* $(,)?
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
            $struct_name, $error_type, $first_version, $(($variant, $path_to_json)),*
        );
    };
}

#[macro_export]
macro_rules! define_versioned_constants_inner {
    (
        $struct_name:ident,
        $error_type:ident,
        $first_version:expr,
        $(($variant:ident, $path_to_json:expr)),* $(,)?
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
    };
}
