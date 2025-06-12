#[macro_export]
macro_rules! define_hint_enum_base {
    ($enum_name:ident, $(($hint_name:ident, $hint_str:expr)),+ $(,)?) => {
        #[cfg_attr(
            any(test, feature = "testing"),
            derive(Default, Serialize, strum_macros::EnumIter)
        )]
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
            #[allow(clippy::result_large_err)]
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
macro_rules! define_stateless_hint_enum {
    ($enum_name:ident, $(($hint_name:ident, $implementation:ident, $hint_str:expr)),+ $(,)?) => {

        $crate::define_hint_enum_base!($enum_name, $(($hint_name, $hint_str)),+);

        impl $enum_name {
            #[allow(clippy::result_large_err)]
            pub(crate) fn execute_hint<'program, CHP: CommonHintProcessor<'program>>(
                &self,
                _hint_processor: &mut CHP,
                hint_args: HintArgs<'_,>
            ) -> OsHintResult {
                match self {
                    $(Self::$hint_name => {
                        #[cfg(feature="testing")]
                        _hint_processor.get_unused_hints().remove(&Self::$hint_name.into());
                        $implementation(hint_args)
                    })+

                }
            }
        }
    };
}

#[macro_export]
macro_rules! define_common_hint_enum {
    ($enum_name:ident, $(($hint_name:ident, $implementation:ident, $hint_str:expr)),+ $(,)?) => {

        $crate::define_hint_enum_base!($enum_name, $(($hint_name, $hint_str)),+);

        impl $enum_name {
            #[allow(clippy::result_large_err)]
            pub(crate) fn execute_hint<'program, CHP: CommonHintProcessor<'program>>(
                &self,
                hint_processor: &mut CHP,
                hint_args: HintArgs<'_>
            ) -> OsHintResult {
                match self {
                    $(Self::$hint_name => {
                        #[cfg(feature="testing")]
                        hint_processor.get_unused_hints().remove(&Self::$hint_name.into());
                        $implementation(hint_processor, hint_args)
                    })+

                }
            }
        }
    };
}

#[macro_export]
macro_rules! define_hint_enum {
    ($enum_name:ident, $(($hint_name:ident, $implementation:ident, $hint_str:expr)),+ $(,)?) => {

        $crate::define_hint_enum_base!($enum_name, $(($hint_name, $hint_str)),+);

        impl $enum_name {
            #[allow(clippy::result_large_err)]
            pub fn execute_hint<S: StateReader>(
                &self,
                hint_processor: &mut SnosHintProcessor<'_, S>,
                hint_args: HintArgs<'_>
            ) -> OsHintResult {
                match self {
                    $(Self::$hint_name => {
                        #[cfg(feature="testing")]
                        hint_processor.unused_hints.remove(&Self::$hint_name.into());
                        $implementation::<S>(hint_processor, hint_args)
                    })+

                }
            }
        }
    };
}

/// Hint extensions extend the current map of hints used by the VM.
/// This behavior achieves what the `vm_load_data` primitive does for cairo-lang and is needed to
/// implement OS hints like `vm_load_program`.
#[macro_export]
macro_rules! define_hint_extension_enum {
    ($enum_name:ident, $(($hint_name:ident, $implementation:ident, $hint_str:expr)),+ $(,)?) => {

        $crate::define_hint_enum_base!($enum_name, $(($hint_name, $hint_str)),+);

        impl $enum_name {
            #[allow(clippy::result_large_err)]
            pub fn execute_hint_extensive<S: StateReader>(
                &self,
                hint_processor: &mut SnosHintProcessor<'_, S>,
                hint_extension_args: HintArgs<'_>,
            ) -> OsHintExtensionResult {
                match self {
                    $(Self::$hint_name => {
                        #[cfg(feature="testing")]
                            hint_processor
                            .unused_hints
                            .remove(&Self::$hint_name.into());
                        $implementation::<S>(hint_processor, hint_extension_args)
                    })+
                }
            }
        }
    };
}
