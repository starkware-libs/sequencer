//! Utils for serializing config objects into flatten map and json file.
//! The elements structure is:
//!
//! ```json
//! "conf1.conf2.conf3.param_name": {
//!     "description": "Param description.",
//!     "value": json_value
//! }
//! ```
//! In addition, supports pointers in the map, with the structure:
//!
//! ```json
//! "conf1.conf2.conf3.param_name": {
//!     "description": "Param description.",
//!     "pointer_target": "target_param_path"
//! }
//! ```
//!
//! Supports required params. A required param has no default value, but the type of value that the
//! user must set:
//! ```json
//! "conf1.conf2.conf3.param_name: {
//!     "description": "Param description.",
//!     "required_type": Number
//! }
//! ```
//!
//! Supports flags for optional params and sub-configs. An optional param / sub-config has an
//! "#is_none" indicator that determines whether to take its value or to deserialize it to None:
//! ```json
//! "conf1.conf2.#is_none": {
//!     "description": "Flag for an optional field.",
//!     "value": true
//! }
//! ```

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufWriter, Write};

use itertools::chain;
use serde::Serialize;
use serde_json::{json, Value};

use crate::{
    ConfigError,
    ParamPath,
    ParamPrivacy,
    ParamPrivacyInput,
    SerializationType,
    SerializedContent,
    SerializedParam,
    FIELD_SEPARATOR,
    IS_NONE_MARK,
};

/// Serialization for configs.
pub trait SerializeConfig {
    /// Conversion of a configuration to a mapping of flattened parameters to their descriptions and
    /// values.
    /// Note, in the case of a None sub configs, its elements will not included in the flatten map.
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam>;

    /// Serialization of a configuration into a JSON file.
    /// Takes a vector of {target pointer params, SerializedParam, and vector of pointing params},
    /// adds the target pointer params with the description and a value, and replaces the value of
    /// the pointing params to contain only the name of the target they point to.
    ///
    /// # Example
    ///
    /// ```
    /// # use std::collections::BTreeMap;
    ///
    /// # use papyrus_config::dumping::{ser_param, SerializeConfig};
    /// # use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
    /// # use serde::{Deserialize, Serialize};
    /// # use tempfile::TempDir;
    ///
    /// #[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
    /// struct ConfigExample {
    ///     key: usize,
    /// }
    ///
    /// impl SerializeConfig for ConfigExample {
    ///     fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
    ///         BTreeMap::from([ser_param(
    ///             "key",
    ///             &self.key,
    ///             "This is key description.",
    ///             ParamPrivacyInput::Public,
    ///         )])
    ///     }
    /// }
    ///
    /// let dir = TempDir::new().unwrap();
    /// let file_path = dir.path().join("config.json");
    /// ConfigExample { key: 42 }.dump_to_file(&vec![], file_path.to_str().unwrap());
    /// ```
    /// Note, in the case of a None sub configs, its elements will not be included in the file.
    fn dump_to_file(
        &self,
        config_pointers: &Vec<(ParamPath, SerializedParam)>,
        file_path: &str,
    ) -> Result<(), ConfigError> {
        let combined_map = combine_config_map_and_pointers(self.dump(), config_pointers)?;

        // Create file writer.
        let file = File::create(file_path)?;
        let mut writer = BufWriter::new(file);

        // Add config as JSON content to writer.
        serde_json::to_writer_pretty(&mut writer, &combined_map)?;

        // Add an extra newline after the JSON content.
        writer.write_all(b"\n")?;

        // Write to file.
        writer.flush()?;
        Ok(())
    }
}

/// Appends `sub_config_name` to the ParamPath for each entry in `sub_config_dump`.
/// In order to load from a dump properly, `sub_config_name` must match the field's name for the
/// struct this function is called from.
pub fn append_sub_config_name(
    sub_config_dump: BTreeMap<ParamPath, SerializedParam>,
    sub_config_name: &str,
) -> BTreeMap<ParamPath, SerializedParam> {
    BTreeMap::from_iter(
        sub_config_dump.into_iter().map(|(field_name, val)| {
            (format!("{sub_config_name}{FIELD_SEPARATOR}{field_name}"), val)
        }),
    )
}

// Serializes a parameter of a config.
fn common_ser_param(
    name: &str,
    content: SerializedContent,
    description: &str,
    privacy: ParamPrivacy,
) -> (String, SerializedParam) {
    (name.to_owned(), SerializedParam { description: description.to_owned(), content, privacy })
}

/// Serializes a single param of a config.
/// The returned pair is designed to be an input to a dumped config map.
pub fn ser_param<T: Serialize>(
    name: &str,
    value: &T,
    description: &str,
    privacy: ParamPrivacyInput,
) -> (String, SerializedParam) {
    common_ser_param(
        name,
        SerializedContent::DefaultValue(json!(value)),
        description,
        privacy.into(),
    )
}

/// Serializes expected type for a single required param of a config.
/// The returned pair is designed to be an input to a dumped config map.
pub fn ser_required_param(
    name: &str,
    serialization_type: SerializationType,
    description: &str,
    privacy: ParamPrivacyInput,
) -> (String, SerializedParam) {
    common_ser_param(
        name,
        SerializedContent::ParamType(serialization_type),
        required_param_description(description).as_str(),
        privacy.into(),
    )
}

/// Serializes expected type for a single param of a config that the system may generate. The
/// generation should be defined as serde default field attribute.
/// The returned pair is designed to be an input to a dumped config map.
pub fn ser_generated_param(
    name: &str,
    serialization_type: SerializationType,
    description: &str,
    privacy: ParamPrivacyInput,
) -> (String, SerializedParam) {
    common_ser_param(
        name,
        SerializedContent::ParamType(serialization_type),
        format!("{} If no value is provided, the system will generate one.", description).as_str(),
        privacy.into(),
    )
}

/// Serializes optional sub-config fields (or default fields for None sub-config) and adds an
/// "#is_none" flag.
pub fn ser_optional_sub_config<T: SerializeConfig + Default>(
    optional_config: &Option<T>,
    name: &str,
) -> BTreeMap<ParamPath, SerializedParam> {
    chain!(
        BTreeMap::from_iter([ser_is_param_none(name, optional_config.is_none())]),
        append_sub_config_name(
            match optional_config {
                None => T::default().dump(),
                Some(config) => config.dump(),
            },
            name,
        ),
    )
    .collect()
}

/// Serializes optional param value (or default value for None param) and adds an "#is_none" flag.
pub fn ser_optional_param<T: Serialize>(
    optional_param: &Option<T>,
    default_value: T,
    name: &str,
    description: &str,
    privacy: ParamPrivacyInput,
) -> BTreeMap<ParamPath, SerializedParam> {
    BTreeMap::from([
        ser_is_param_none(name, optional_param.is_none()),
        ser_param(
            name,
            match optional_param {
                Some(param) => param,
                None => &default_value,
            },
            description,
            privacy,
        ),
    ])
}

/// Serializes is_none flag for a param.
pub fn ser_is_param_none(name: &str, is_none: bool) -> (String, SerializedParam) {
    common_ser_param(
        format!("{name}{FIELD_SEPARATOR}{IS_NONE_MARK}").as_str(),
        SerializedContent::DefaultValue(json!(is_none)),
        "Flag for an optional field.",
        ParamPrivacy::TemporaryValue,
    )
}

/// Serializes a pointer target param of a config.
///
/// # Example
/// Create config_pointers vector to be used in `dump_to_file`:
/// ```
/// # use papyrus_config::dumping::ser_pointer_target_param;
///
/// let pointer_target_param = ser_pointer_target_param(
///     "shared_param",
///     &("param".to_string()),
///     "A string parameter description.",
/// );
/// let pointer_param_paths =
///     vec!["conf1.conf2.same_param".to_owned(), "conf3.same_param".to_owned()];
/// let config_pointers = vec![(pointer_target_param, pointer_param_paths)];
/// ```
pub fn ser_pointer_target_param<T: Serialize>(
    name: &str,
    value: &T,
    description: &str,
) -> (String, SerializedParam) {
    common_ser_param(
        name,
        SerializedContent::DefaultValue(json!(value)),
        description,
        ParamPrivacy::TemporaryValue,
    )
}

/// Serializes a pointer target for a required param of a config.
pub fn ser_pointer_target_required_param(
    name: &str,
    serialization_type: SerializationType,
    description: &str,
) -> (String, SerializedParam) {
    common_ser_param(
        name,
        SerializedContent::ParamType(serialization_type),
        required_param_description(description).as_str(),
        ParamPrivacy::TemporaryValue,
    )
}

/// Takes a config map and a vector of target parameters with their serialized representations.
/// Adds each target param to the config map.
/// Updates entries in the map to point to these targets, replacing values of entries that match
/// the target parameter paths to contain only the name of the target they point to.
pub(crate) fn combine_config_map_and_pointers(
    mut config_map: BTreeMap<ParamPath, SerializedParam>,
    pointers: &Vec<(ParamPath, SerializedParam)>,
) -> Result<Value, ConfigError> {
    // Update config with target params.
    for (target_param, serialized_pointer) in pointers {
        // Insert target param.
        config_map.insert(target_param.clone(), serialized_pointer.clone());

        // Update config entries that match the target param as pointers.
        config_map.iter_mut().for_each(|(param_path, serialized_param)| {
            // Check if the param is the target param.
            if param_path.ends_with(format!("{FIELD_SEPARATOR}{target_param}").as_str()) {
                // Point to the target param.
                *serialized_param = SerializedParam {
                    description: serialized_param.description.clone(),
                    content: SerializedContent::PointerTarget(target_param.to_owned()),
                    privacy: serialized_param.privacy.clone(),
                };
            }
        });
    }

    Ok(json!(config_map))
}

pub(crate) fn required_param_description(description: &str) -> String {
    format!("A required param! {}", description)
}
