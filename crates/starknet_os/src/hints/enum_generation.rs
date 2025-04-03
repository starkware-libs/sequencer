/// Common code generation for `define_hint_enum` and `define_hint_extension_enum`.
#[macro_export]
macro_rules! define_hint_enum_base {
    ($enum_name:ident, $(($hint_name:ident, $hint_str:expr)),+ $(,)?) => {
        #[cfg_attr(any(test, feature = "testing"), derive(Default, strum_macros::EnumIter))]
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
        pub enum $enum_name {
            // Make first variant the default variant for testing (iteration) purposes.
            #[cfg_attr(any(test, feature = "testing"), default)]
            $($hint_name),+
        }

        impl From<$enum_name> for AllHints {
            fn from(hint: $enum_name) -> Self {
                Self::$enum_name(hint)
            }
        }

        impl HintEnum for $enum_name {
            fn from_str(hint_str: &str) -> Result<Self, HintImplementationError> {
                match hint_str {
                    $($hint_str => Ok(Self::$hint_name),)+
                    _ => Err(HintImplementationError::UnknownHint(hint_str.to_string())),
                }
            }

            fn to_str(&self) -> &'static str {
                match self {
                    $(Self::$hint_name => $hint_str,)+
                }
            }
        }
    }
}

/// Generates the implementation of the `HintImplementation` trait or the
/// `HintExtensionImplementation` trait for the given enum.
#[macro_export]
macro_rules! generate_hint_implementation_block {
    (
        $enum_name:ident,
        $trait:ty,
        $method_name:ident,
        $return_type:ty,
        $(($hint_name:ident, $implementation:ident)),+ $(,)?
    ) => {
        impl $trait for $enum_name {
            fn $method_name<S: StateReader>(
                &self, hint_args: HintArgs<'_, S>
            ) -> $return_type {
                match self {
                    $(
                        Self::$hint_name => $implementation::<S>(hint_args)
                            .map_err(|error| HintImplementationError::OsHint {
                                hint: AllHints::$enum_name(*self),
                                error
                            }),
                    )+
                }
            }
        }
    }
}

/// Defines the different hints that can be used in the OS program, and generates the implementation
/// of the `HintImplementation` trait. Expects a tuple of the hint name, the implementation
/// function, and the hint string.
#[macro_export]
macro_rules! define_hint_enum {
    ($enum_name:ident, $(($hint_name:ident, $implementation:ident, $hint_str:expr)),+ $(,)?) => {
        $crate::define_hint_enum_base!($enum_name, $(($hint_name, $hint_str)),+);
        $crate::generate_hint_implementation_block!(
            $enum_name,
            HintImplementation,
            execute_hint,
            HintImplementationResult,
            $(($hint_name, $implementation)),+
        );
    };
}

/// Defines the different hints that can be used in the OS program, and generates the implementation
/// of the `HintExtensionImplementation` trait. Expects a tuple of the hint name, the implementation
/// function, and the hint string.
#[macro_export]
macro_rules! define_hint_extension_enum {
    ($enum_name:ident, $(($hint_name:ident, $implementation:ident, $hint_str:expr)),+ $(,)?) => {
        $crate::define_hint_enum_base!($enum_name, $(($hint_name, $hint_str)),+);
        $crate::generate_hint_implementation_block!(
            $enum_name,
            HintExtensionImplementation,
            execute_hint_extensive,
            HintExtensionImplementationResult,
            $(($hint_name, $implementation)),+
        );
    };
}
