#[macro_export]
macro_rules! define_hint_enum_base {
    ($enum_name:ident, $(($hint_name:ident, $hint_str:expr)),+ $(,)?) => {
        #[cfg_attr(any(test, feature = "testing"), derive(Default, strum_macros::EnumIter))]
        #[derive(Debug, PartialEq)]
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
            fn from_str(hint_str: &str) -> Result<Self, OsHintError> {
                match hint_str {
                    $($hint_str => Ok(Self::$hint_name),)+
                    _ => Err(OsHintError::UnknownHint(hint_str.to_string())),
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

#[macro_export]
macro_rules! define_hint_enum {
    ($enum_name:ident, $(($hint_name:ident, $implementation:ident, $hint_str:expr)),+ $(,)?) => {

        $crate::define_hint_enum_base!($enum_name, $(($hint_name, $hint_str)),+);

        impl HintImplementation for $enum_name {
            fn execute_hint<S: StateReader>(&self, hint_args: HintArgs<'_, S>) -> HintResult {
                match self {
                    $(Self::$hint_name => $implementation::<S>(hint_args),)+
                }
            }
        }
    };
}

#[macro_export]
macro_rules! define_hint_extension_enum {
    ($enum_name:ident, $(($hint_name:ident, $implementation:ident, $hint_str:expr)),+ $(,)?) => {

        $crate::define_hint_enum_base!($enum_name, $(($hint_name, $hint_str)),+);

        impl HintExtensionImplementation for $enum_name {
            fn execute_hint_extensive<S: StateReader>(
                &self,
                hint_extension_args: HintArgs<'_, S>,
            ) -> HintExtensionResult {
                match self {
                    $(Self::$hint_name => $implementation::<S>(hint_extension_args),)+
                }
            }
        }
    };
}
