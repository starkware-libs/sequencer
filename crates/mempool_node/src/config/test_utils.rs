use std::vec::Vec; // Used by #[gen_field_names_fn].

use papyrus_config::{ParamPath, SerializationType, SerializedContent, SerializedParam};
use papyrus_proc_macros::gen_field_names_fn;
use starknet_api::core::ChainId;

/// Required parameters utility struct.
#[gen_field_names_fn]
pub struct RequiredParams {
    pub chain_id: ChainId,
}

impl RequiredParams {
    pub fn create_for_testing() -> Self {
        Self { chain_id: ChainId::create_for_testing() }
    }
}

// TODO(Tsabary): Bundle required config values in a struct, detailing whether they are pointer
// targets or not. Then, derive their values in the config (struct and pointers).
// Also, add functionality to derive them for testing.
// Creates a vector of strings with the command name and required parameters that can be used as
// arguments to load a config.
#[cfg(any(feature = "testing", test))]
pub fn create_test_config_load_args(pointers: &Vec<(ParamPath, SerializedParam)>) -> Vec<String> {
    use crate::config::node_command;

    let mut dummy_values = Vec::new();

    // Command name.
    dummy_values.push(node_command().to_string());

    // Iterate over required config parameters and add them as args with suitable arbitrary values.
    for (target_param, serialized_pointer) in pointers {
        // Param name.
        let required_param_name_as_arg = format!("--{}", target_param);
        dummy_values.push(required_param_name_as_arg);

        // Param value.
        let serialization_type = match &serialized_pointer.content {
            SerializedContent::ParamType(serialization_type) => serialization_type,
            _ => panic!("Required parameters have to be of type ParamType."),
        };
        let arbitrary_value = match serialization_type {
            SerializationType::Boolean => "false",
            SerializationType::Float => "15.2",
            SerializationType::NegativeInteger => "-30",
            SerializationType::PositiveInteger => "17",
            SerializationType::String => "ArbitraryString",
        }
        .to_string();
        dummy_values.push(arbitrary_value);
    }
    dummy_values
}
