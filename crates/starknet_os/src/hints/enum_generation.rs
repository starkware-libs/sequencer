#[macro_export]
macro_rules! define_hint_enum_base {
    ($enum_name:ident, $(($hint_name:ident, $hint_str:expr)),+ $(,)?) => {
        #[cfg_attr(any(test, feature = "testing"), derive(strum_macros::EnumIter))]
        pub enum $enum_name {
            $($hint_name),+
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
            fn execute_hint(
                &self,
                vm: &mut VirtualMachine,
                exec_scopes: &mut ExecutionScopes,
                ids_data: &HashMap<String, HintReference>,
                ap_tracking: &ApTracking,
                constants: &HashMap<String, Felt>,
            ) -> HintResult {
                match self {
                    $(Self::$hint_name => $implementation(
                        vm, exec_scopes, ids_data, ap_tracking, constants
                    ),)+
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
            fn execute_hint_extensive(
                &self,
                hint_processor: &dyn HintProcessor,
                vm: &mut VirtualMachine,
                exec_scopes: &mut ExecutionScopes,
                ids_data: &HashMap<String, HintReference>,
                ap_tracking: &ApTracking,
            ) -> HintExtensionResult {
                match self {
                    $(Self::$hint_name => $implementation(
                        hint_processor, vm, exec_scopes, ids_data, ap_tracking
                    ),)+
                }
            }
        }
    };
}
