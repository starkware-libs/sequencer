/// Auto-generate getters for listed versioned constants versions.
/// Optionally provide an intermediate struct for deserialization.
#[macro_export]
macro_rules! define_versioned_constants {
    ($struct_name:ident, $error_type:ident, $(($variant:ident, $path_to_json:expr)),* $(,)?) => {
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
            $struct_name, $error_type, $(($variant, $path_to_json)),*
        );
    };

    (
        $struct_name:ident,
        $intermediate_struct_name:ident,
        $error_type:ident,
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
            $struct_name, $error_type, $(($variant, $path_to_json)),*
        );
    };
}

#[macro_export]
macro_rules! define_versioned_constants_inner {
    ($struct_name:ident, $error_type:ident, $(($variant:ident, $path_to_json:expr)),* $(,)?) => {
        paste::paste! {
            $(
                pub(crate) const [<VERSIONED_CONSTANTS_ $variant:upper _JSON>]: &str =
                    include_str!($path_to_json);
            )*
        }

        impl $struct_name {
            /// Gets the path to the JSON file for the specified Starknet version.
            pub fn json_str(version: &starknet_api::block::StarknetVersion) -> Result<&'static str, $error_type> {
                match version {
                    $(starknet_api::block::StarknetVersion::$variant => paste::paste! {
                        Ok([<VERSIONED_CONSTANTS_ $variant:upper _JSON>])
                    },)*
                    _ => Err($error_type::InvalidStarknetVersion(*version)),
                }
            }

            /// Gets the constants that shipped with the current version of the Starknet.
            /// To use custom constants, initialize the struct from a file using `from_path`.
            pub fn latest_constants() -> &'static Self {
                Self::get(&starknet_api::block::StarknetVersion::LATEST)
                    .expect("Latest version should support VC.")
            }

            /// Gets the constants for the specified Starknet version.
            pub fn get(version: &starknet_api::block::StarknetVersion) -> Result<&'static Self, $error_type> {
                match version {
                    $(
                        starknet_api::block::StarknetVersion::$variant => Ok(
                            & paste::paste! { [<VERSIONED_CONSTANTS_ $variant:upper>] }
                        ),
                    )*
                    _ => Err($error_type::InvalidStarknetVersion(*version)),
                }
            }
        }

        /// Gets a string of the constants of the latest version of Starknet.
        pub static VERSIONED_CONSTANTS_LATEST_JSON: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
            let latest_variant = StarknetVersion::LATEST;
            $struct_name::json_str(&latest_variant)
                .expect("Latest version should support VC.").to_string()
        });
    };
}
