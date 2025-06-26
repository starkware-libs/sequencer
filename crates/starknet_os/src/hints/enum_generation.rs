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
macro_rules! define_hint_enum_helper {
    (
        $enum_name:ident,
        $hp_arg:ident,
        $(($hint_name:ident, $implementation:ident, $hint_str:expr $(, $passed_arg:ident)?)),+ $(,)?
    ) => {

        $crate::define_hint_enum_base!($enum_name, $(($hint_name, $hint_str)),+);

        impl $enum_name {
            pub(crate) fn execute_hint<'program, CHP: CommonHintProcessor<'program>>(
                &self,
                $hp_arg: &mut CHP,
                hint_args: HintArgs<'_>
            ) -> OsHintResult {
                match self {
                    $(Self::$hint_name => {
                        #[cfg(any(test, feature = "testing"))]
                        $hp_arg.get_unused_hints().remove(&Self::$hint_name.into());
                        $implementation($($passed_arg, )? hint_args)
                    })+
                }
            }
        }
    };
}

#[macro_export]
macro_rules! define_stateless_hint_enum {
    ($enum_name:ident, $(($hint_name:ident, $implementation:ident, $hint_str:expr)),+ $(,)?) => {
        $crate::define_hint_enum_helper!(
            $enum_name,
            _hint_processor,
            $(($hint_name, $implementation, $hint_str)),+
        );
    };
}

#[macro_export]
macro_rules! define_common_hint_enum {
    ($enum_name:ident, $(($hint_name:ident, $implementation:ident, $hint_str:expr)),+ $(,)?) => {
        $crate::define_hint_enum_helper!(
            $enum_name,
            hint_processor,
            $(($hint_name, $implementation, $hint_str, hint_processor)),+
        );
    };
}

#[macro_export]
macro_rules! define_hint_enum {
    (
        $enum_name:ident,
        $hp: ty
        $(, $generic_var:ident, $generic:ident)?,
        $(($hint_name:ident, $implementation:ident, $hint_str:expr)),+ $(,)?
    ) => {

        $crate::define_hint_enum_base!($enum_name, $(($hint_name, $hint_str)),+);

        impl $enum_name {
<<<<<<< HEAD
            pub fn execute_hint<S: StateReader>(
||||||| 2452f56bc
            #[allow(clippy::result_large_err)]
            pub fn execute_hint<S: StateReader>(
=======
            pub fn execute_hint$(<$generic_var: $generic>)?(
>>>>>>> origin/main-v0.14.0
                &self,
                hint_processor: &mut $hp,
                hint_args: HintArgs<'_>
            ) -> OsHintResult {
                match self {
                    $(Self::$hint_name => {
                        #[cfg(any(test, feature = "testing"))]
                        hint_processor.unused_hints.remove(&Self::$hint_name.into());
                        $implementation(hint_processor, hint_args)
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
            pub fn execute_hint_extensive<S: StateReader>(
                &self,
                hint_processor: &mut SnosHintProcessor<'_, S>,
                hint_extension_args: HintArgs<'_>,
            ) -> OsHintExtensionResult {
                match self {
                    $(Self::$hint_name => {
                        #[cfg(any(test, feature = "testing"))]
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
