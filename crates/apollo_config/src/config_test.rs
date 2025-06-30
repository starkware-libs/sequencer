use std::collections::{BTreeMap, HashMap, HashSet};
use std::env;
use std::fs::File;
use std::path::PathBuf;
use std::time::Duration;

use apollo_infra_utils::path::resolve_project_relative_path;
use assert_matches::assert_matches;
use clap::Command;
use itertools::chain;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tempfile::{NamedTempFile, TempDir};
use validator::Validate;

use crate::command::{get_command_matches, update_config_map_by_command_args};
use crate::converters::{
    deserialize_milliseconds_to_duration,
    deserialize_optional_list_with_nested_json,
    serialize_optional_list_with_nested_json,
};
use crate::dumping::{
    combine_config_map_and_pointers,
    generate_struct_pointer,
    prepend_sub_config_name,
    required_param_description,
    ser_generated_param,
    ser_optional_param,
    ser_optional_sub_config,
    ser_param,
    ser_pointer_target_param,
    ser_pointer_target_required_param,
    ser_required_param,
    set_pointing_param_paths,
    SerializeConfig,
};
use crate::loading::{
    load,
    load_and_process_config,
    split_pointers_map,
    split_values_and_types,
    update_config_map_by_pointers,
    update_optional_values,
};
use crate::presentation::get_config_presentation;
use crate::{
    ConfigError,
    ParamPath,
    ParamPrivacy,
    ParamPrivacyInput,
    SerializationType,
    SerializedContent,
    SerializedParam,
    CONFIG_FILE_ARG,
};

lazy_static! {
    static ref CUSTOM_CONFIG_PATH: PathBuf =
        resolve_project_relative_path("crates/apollo_config/resources/custom_config_example.json")
            .unwrap();
}

#[derive(Clone, Copy, Default, Serialize, Deserialize, Debug, PartialEq, Validate)]
struct InnerConfig {
    #[validate(range(min = 0, max = 10))]
    o: usize,
}

impl SerializeConfig for InnerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([ser_param("o", &self.o, "This is o.", ParamPrivacyInput::Public)])
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Validate)]
struct OuterConfig {
    opt_elem: Option<usize>,
    opt_config: Option<InnerConfig>,
    #[validate]
    inner_config: InnerConfig,
}

impl SerializeConfig for OuterConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        chain!(
            ser_optional_param(
                &self.opt_elem,
                1,
                "opt_elem",
                "This is elem.",
                ParamPrivacyInput::Public
            ),
            ser_optional_sub_config(&self.opt_config, "opt_config"),
            prepend_sub_config_name(self.inner_config.dump(), "inner_config"),
        )
        .collect()
    }
}

#[test]
fn dump_and_load_config() {
    let some_outer_config = OuterConfig {
        opt_elem: Some(2),
        opt_config: Some(InnerConfig { o: 3 }),
        inner_config: InnerConfig { o: 4 },
    };
    let none_outer_config =
        OuterConfig { opt_elem: None, opt_config: None, inner_config: InnerConfig { o: 5 } };

    for outer_config in [some_outer_config, none_outer_config] {
        let (mut dumped, _) = split_values_and_types(outer_config.dump());
        update_optional_values(&mut dumped);
        let loaded_config = load::<OuterConfig>(&dumped).unwrap();
        assert_eq!(loaded_config, outer_config);
    }
}

#[test]
fn test_validation() {
    let outer_config =
        OuterConfig { opt_elem: None, opt_config: None, inner_config: InnerConfig { o: 20 } };
    assert!(outer_config.validate().is_err());
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
struct TypicalConfig {
    #[serde(deserialize_with = "deserialize_milliseconds_to_duration")]
    a: Duration,
    b: String,
    c: bool,
    d: i64,
    e: u64,
    f: f64,
}

impl SerializeConfig for TypicalConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([
            ser_param(
                "a",
                &self.a.as_millis(),
                "This is a as milliseconds.",
                ParamPrivacyInput::Public,
            ),
            ser_param("b", &self.b, "This is b.", ParamPrivacyInput::Public),
            ser_param("c", &self.c, "This is c.", ParamPrivacyInput::Private),
            ser_param("d", &self.d, "This is d.", ParamPrivacyInput::Public),
            ser_param("e", &self.e, "This is e.", ParamPrivacyInput::Public),
            ser_param("f", &self.f, "This is f.", ParamPrivacyInput::Public),
        ])
    }
}

#[test]
fn test_update_dumped_config() {
    let command = Command::new("Testing");
    let dumped_config = TypicalConfig {
        a: Duration::from_secs(1),
        b: "bbb".to_owned(),
        c: false,
        d: -1,
        e: 10,
        f: 1.5,
    }
    .dump();
    let args = vec!["Testing", "--a", "1234", "--b", "15", "--d", "-2", "--e", "20", "--f", "0.5"];
    env::set_var("C", "true");
    let args: Vec<String> = args.into_iter().map(|s| s.to_owned()).collect();

    let arg_matches = get_command_matches(&dumped_config, command, args).unwrap();
    let (mut config_map, required_map) = split_values_and_types(dumped_config);
    update_config_map_by_command_args(&mut config_map, &required_map, &arg_matches).unwrap();

    assert_eq!(json!(1234), config_map["a"]);
    assert_eq!(json!("15"), config_map["b"]);
    assert_eq!(json!(true), config_map["c"]);
    assert_eq!(json!(-2), config_map["d"]);
    assert_eq!(json!(20), config_map["e"]);
    assert_eq!(json!(0.5), config_map["f"]);

    let loaded_config: TypicalConfig = load(&config_map).unwrap();
    assert_eq!(Duration::from_millis(1234), loaded_config.a);
}

#[test]
fn test_env_nested_params() {
    let command = Command::new("Testing");
    let dumped_config = OuterConfig {
        opt_elem: Some(1),
        opt_config: Some(InnerConfig { o: 2 }),
        inner_config: InnerConfig { o: 3 },
    }
    .dump();
    let args = vec!["Testing", "--opt_elem", "1234"];
    env::set_var("OPT_CONFIG____IS_NONE__", "true");
    env::set_var("INNER_CONFIG__O", "4");
    let args: Vec<String> = args.into_iter().map(|s| s.to_owned()).collect();

    let arg_matches = get_command_matches(&dumped_config, command, args).unwrap();
    let (mut config_map, required_map) = split_values_and_types(dumped_config);
    update_config_map_by_command_args(&mut config_map, &required_map, &arg_matches).unwrap();

    assert_eq!(json!(1234), config_map["opt_elem"]);
    assert_eq!(json!(true), config_map["opt_config.#is_none"]);
    assert_eq!(json!(4), config_map["inner_config.o"]);

    update_optional_values(&mut config_map);

    let loaded_config: OuterConfig = load(&config_map).unwrap();
    assert_eq!(Some(1234), loaded_config.opt_elem);
    assert_eq!(None, loaded_config.opt_config);
    assert_eq!(4, loaded_config.inner_config.o);
}

#[test]
fn test_config_presentation() {
    let config = TypicalConfig {
        a: Duration::from_secs(1),
        b: "bbb".to_owned(),
        c: false,
        d: -1,
        e: 10,
        f: 0.5,
    };
    let presentation = get_config_presentation(&config, true).unwrap();
    let keys: Vec<_> = presentation.as_object().unwrap().keys().collect();
    assert_eq!(keys, vec!["a", "b", "c", "d", "e", "f"]);

    let public_presentation = get_config_presentation(&config, false).unwrap();
    let keys: Vec<_> = public_presentation.as_object().unwrap().keys().collect();
    assert_eq!(keys, vec!["a", "b", "d", "e", "f"]);
}

#[test]
fn test_nested_config_presentation() {
    let configs = vec![
        OuterConfig {
            opt_elem: Some(1),
            opt_config: Some(InnerConfig { o: 2 }),
            inner_config: InnerConfig { o: 3 },
        },
        OuterConfig {
            opt_elem: None,
            opt_config: Some(InnerConfig { o: 2 }),
            inner_config: InnerConfig { o: 3 },
        },
        OuterConfig { opt_elem: Some(1), opt_config: None, inner_config: InnerConfig { o: 3 } },
    ];

    for config in configs {
        let presentation = get_config_presentation(&config, true).unwrap();
        let keys: Vec<_> = presentation.as_object().unwrap().keys().collect();
        assert_eq!(keys, vec!["inner_config", "opt_config", "opt_elem"]);
        let public_presentation = get_config_presentation(&config, false).unwrap();
        let keys: Vec<_> = public_presentation.as_object().unwrap().keys().collect();
        assert_eq!(keys, vec!["inner_config", "opt_config", "opt_elem"]);
    }
}

#[test]
fn test_pointers_flow() {
    const TARGET_PARAM_NAME: &str = "a";
    const TARGET_PARAM_DESCRIPTION: &str = "This is common a.";
    const POINTING_PARAM_DESCRIPTION: &str = "This is a.";
    const PUBLIC_POINTING_PARAM_NAME: &str = "public_a.a";
    const PRIVATE_POINTING_PARAM_NAME: &str = "private_a.a";
    const WHITELISTED_POINTING_PARAM_NAME: &str = "non_pointing.a";
    const VALUE: usize = 5;

    let config_map = BTreeMap::from([
        ser_param(
            PUBLIC_POINTING_PARAM_NAME,
            &json!(VALUE),
            POINTING_PARAM_DESCRIPTION,
            ParamPrivacyInput::Public,
        ),
        ser_param(
            PRIVATE_POINTING_PARAM_NAME,
            &json!(VALUE),
            POINTING_PARAM_DESCRIPTION,
            ParamPrivacyInput::Private,
        ),
        ser_param(
            WHITELISTED_POINTING_PARAM_NAME,
            &json!(VALUE),
            POINTING_PARAM_DESCRIPTION,
            ParamPrivacyInput::Private,
        ),
    ]);
    let pointers = vec![(
        ser_pointer_target_param(TARGET_PARAM_NAME, &json!(10), TARGET_PARAM_DESCRIPTION),
        HashSet::from([
            PUBLIC_POINTING_PARAM_NAME.to_string(),
            PRIVATE_POINTING_PARAM_NAME.to_string(),
        ]),
    )];
    let non_pointer_params = HashSet::from([WHITELISTED_POINTING_PARAM_NAME.to_string()]);
    let stored_map =
        combine_config_map_and_pointers(config_map, &pointers, &non_pointer_params).unwrap();

    // Assert the pointing parameters are correctly set.
    assert_eq!(
        stored_map[PUBLIC_POINTING_PARAM_NAME],
        json!(SerializedParam {
            description: POINTING_PARAM_DESCRIPTION.to_owned(),
            content: SerializedContent::PointerTarget(TARGET_PARAM_NAME.to_owned()),
            privacy: ParamPrivacy::Public,
        })
    );
    assert_eq!(
        stored_map[PRIVATE_POINTING_PARAM_NAME],
        json!(SerializedParam {
            description: POINTING_PARAM_DESCRIPTION.to_owned(),
            content: SerializedContent::PointerTarget(TARGET_PARAM_NAME.to_owned()),
            privacy: ParamPrivacy::Private,
        })
    );

    // Assert the whitelisted parameter is correctly set.
    assert_eq!(
        stored_map[WHITELISTED_POINTING_PARAM_NAME],
        json!(SerializedParam {
            description: POINTING_PARAM_DESCRIPTION.to_owned(),
            content: SerializedContent::DefaultValue(json!(VALUE)),
            privacy: ParamPrivacy::Private,
        })
    );

    // Assert the pointed parameter is correctly set as a required parameter.
    assert_eq!(
        stored_map[TARGET_PARAM_NAME],
        json!(SerializedParam {
            description: TARGET_PARAM_DESCRIPTION.to_owned(),
            content: SerializedContent::DefaultValue(json!(10)),
            privacy: ParamPrivacy::TemporaryValue,
        })
    );
    let serialized = serde_json::to_string(&stored_map).unwrap();
    let loaded = serde_json::from_str(&serialized).unwrap();
    let (loaded_config_map, loaded_pointers_map) = split_pointers_map(loaded);
    let (mut config_map, _) = split_values_and_types(loaded_config_map);
    update_config_map_by_pointers(&mut config_map, &loaded_pointers_map).unwrap();
    assert_eq!(config_map[PUBLIC_POINTING_PARAM_NAME], json!(10));
    assert_eq!(config_map[PUBLIC_POINTING_PARAM_NAME], config_map[PRIVATE_POINTING_PARAM_NAME]);
}

#[test]
fn test_required_pointers_flow() {
    // Set up the config map and pointers.
    const REQUIRED_PARAM_NAME: &str = "b";
    const REQUIRED_PARAM_DESCRIPTION: &str = "This is common required b.";
    const POINTING_PARAM_DESCRIPTION: &str = "This is b.";
    const PUBLIC_POINTING_PARAM_NAME: &str = "public_b.b";
    const PRIVATE_POINTING_PARAM_NAME: &str = "private_b.b";
    const WHITELISTED_POINTING_PARAM_NAME: &str = "non_pointing.b";
    const VALUE: usize = 6;

    let config_map = BTreeMap::from([
        ser_param(
            PUBLIC_POINTING_PARAM_NAME,
            &json!(VALUE),
            POINTING_PARAM_DESCRIPTION,
            ParamPrivacyInput::Public,
        ),
        ser_param(
            PRIVATE_POINTING_PARAM_NAME,
            &json!(VALUE),
            POINTING_PARAM_DESCRIPTION,
            ParamPrivacyInput::Private,
        ),
        ser_param(
            WHITELISTED_POINTING_PARAM_NAME,
            &json!(VALUE),
            POINTING_PARAM_DESCRIPTION,
            ParamPrivacyInput::Private,
        ),
    ]);
    let pointers = vec![(
        ser_pointer_target_required_param(
            REQUIRED_PARAM_NAME,
            SerializationType::PositiveInteger,
            REQUIRED_PARAM_DESCRIPTION,
        ),
        HashSet::from([
            PUBLIC_POINTING_PARAM_NAME.to_string(),
            PRIVATE_POINTING_PARAM_NAME.to_string(),
        ]),
    )];
    let non_pointer_params = HashSet::from([WHITELISTED_POINTING_PARAM_NAME.to_string()]);
    let stored_map =
        combine_config_map_and_pointers(config_map, &pointers, &non_pointer_params).unwrap();

    // Assert the pointing parameters are correctly set.
    assert_eq!(
        stored_map[PUBLIC_POINTING_PARAM_NAME],
        json!(SerializedParam {
            description: POINTING_PARAM_DESCRIPTION.to_owned(),
            content: SerializedContent::PointerTarget(REQUIRED_PARAM_NAME.to_owned()),
            privacy: ParamPrivacy::Public,
        })
    );
    assert_eq!(
        stored_map[PRIVATE_POINTING_PARAM_NAME],
        json!(SerializedParam {
            description: POINTING_PARAM_DESCRIPTION.to_owned(),
            content: SerializedContent::PointerTarget(REQUIRED_PARAM_NAME.to_owned()),
            privacy: ParamPrivacy::Private,
        })
    );

    // Assert the whitelisted parameter is correctly set.
    assert_eq!(
        stored_map[WHITELISTED_POINTING_PARAM_NAME],
        json!(SerializedParam {
            description: POINTING_PARAM_DESCRIPTION.to_owned(),
            content: SerializedContent::DefaultValue(json!(VALUE)),
            privacy: ParamPrivacy::Private,
        })
    );

    // Assert the pointed parameter is correctly set as a required parameter.
    assert_eq!(
        stored_map[REQUIRED_PARAM_NAME],
        json!(SerializedParam {
            description: required_param_description(REQUIRED_PARAM_DESCRIPTION).to_owned(),
            content: SerializedContent::ParamType(SerializationType::PositiveInteger),
            privacy: ParamPrivacy::TemporaryValue,
        })
    );
}

#[test]
#[should_panic(
    expected = "The target param should_be_pointing.c should point to c, or to be whitelisted."
)]
fn test_missing_pointer_flow() {
    const TARGET_PARAM_NAME: &str = "c";
    const TARGET_PARAM_DESCRIPTION: &str = "This is common c.";
    const PARAM_DESCRIPTION: &str = "This is c.";
    const NON_POINTING_PARAM_NAME: &str = "should_be_pointing.c";

    // Define a non-pointing parameter and a target pointer such that the parameter name matches the
    // target.
    let config_map = BTreeMap::from([ser_param(
        NON_POINTING_PARAM_NAME,
        &json!(7),
        PARAM_DESCRIPTION,
        ParamPrivacyInput::Private,
    )]);
    let pointers = vec![(
        ser_pointer_target_param(TARGET_PARAM_NAME, &json!(10), TARGET_PARAM_DESCRIPTION),
        HashSet::new(),
    )];
    // Do not whitelist the non-pointing parameter.
    let non_pointer_params = HashSet::new();

    // Attempt to combine the config map and pointers. This should panic.
    combine_config_map_and_pointers(config_map, &pointers, &non_pointer_params).unwrap();
}

#[test]
fn test_replace_pointers() {
    let (mut config_map, _) = split_values_and_types(BTreeMap::from([ser_param(
        "a",
        &json!(5),
        "This is a.",
        ParamPrivacyInput::Public,
    )]));
    let pointers_map =
        BTreeMap::from([("b".to_owned(), "a".to_owned()), ("c".to_owned(), "a".to_owned())]);
    update_config_map_by_pointers(&mut config_map, &pointers_map).unwrap();
    assert_eq!(config_map["a"], config_map["b"]);
    assert_eq!(config_map["a"], config_map["c"]);

    let err = update_config_map_by_pointers(&mut BTreeMap::default(), &pointers_map).unwrap_err();
    assert_matches!(err, ConfigError::PointerTargetNotFound { .. });
}

#[test]
fn test_struct_pointers() {
    const TARGET_PREFIX: &str = "base";
    let target_value =
        RequiredConfig { param_path: "Not a default param_path.".to_owned(), num: 10 };
    let config_map = StructPointersConfig::default().dump();

    let pointers = generate_struct_pointer(
        TARGET_PREFIX.to_owned(),
        &target_value,
        set_pointing_param_paths(&["a", "b"]),
    );
    let stored_map =
        combine_config_map_and_pointers(config_map, &pointers, &HashSet::default()).unwrap();

    // Assert the pointing parameters are correctly set.
    assert_eq!(
        stored_map["a.param_path"],
        json!(SerializedParam {
            description: required_param_description(RequiredConfig::param_path_description())
                .to_owned(),
            content: SerializedContent::PointerTarget(
                format!("{TARGET_PREFIX}.param_path").to_owned()
            ),
            privacy: ParamPrivacy::Public,
        })
    );
    assert_eq!(
        stored_map["a.num"],
        json!(SerializedParam {
            description: RequiredConfig::num_description().to_owned(),
            content: SerializedContent::PointerTarget(format!("{TARGET_PREFIX}.num").to_owned()),
            privacy: ParamPrivacy::Public,
        })
    );
    assert_eq!(
        stored_map["b.param_path"],
        json!(SerializedParam {
            description: required_param_description(RequiredConfig::param_path_description())
                .to_owned(),
            content: SerializedContent::PointerTarget(
                format!("{TARGET_PREFIX}.param_path").to_owned()
            ),
            privacy: ParamPrivacy::Public,
        })
    );
    assert_eq!(
        stored_map["b.num"],
        json!(SerializedParam {
            description: RequiredConfig::num_description().to_owned(),
            content: SerializedContent::PointerTarget(format!("{TARGET_PREFIX}.num").to_owned()),
            privacy: ParamPrivacy::Public,
        })
    );

    // Assert the pointed parameter is correctly set.
    assert_eq!(
        stored_map[format!("{TARGET_PREFIX}.param_path").to_owned()],
        json!(SerializedParam {
            description: required_param_description(RequiredConfig::param_path_description())
                .to_owned(),
            content: SerializedContent::ParamType(SerializationType::String),
            privacy: ParamPrivacy::TemporaryValue,
        })
    );
    assert_eq!(
        stored_map[format!("{TARGET_PREFIX}.num").to_owned()],
        json!(SerializedParam {
            description: RequiredConfig::num_description().to_owned(),
            content: SerializedContent::DefaultValue(json!(10)),
            privacy: ParamPrivacy::TemporaryValue,
        })
    );
}

#[derive(Clone, Default, Serialize, Deserialize, Debug, PartialEq)]
struct StructPointersConfig {
    pub a: RequiredConfig,
    pub b: RequiredConfig,
}
impl SerializeConfig for StructPointersConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut dump = BTreeMap::new();
        dump.append(&mut prepend_sub_config_name(self.a.dump(), "a"));
        dump.append(&mut prepend_sub_config_name(self.b.dump(), "b"));
        dump
    }
}

#[derive(Clone, Default, Serialize, Deserialize, Debug, PartialEq)]
struct CustomConfig {
    param_path: String,
    #[serde(default)]
    seed: usize,
}

impl SerializeConfig for CustomConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([
            ser_param(
                "param_path",
                &self.param_path,
                "This is param_path.",
                ParamPrivacyInput::Public,
            ),
            ser_generated_param(
                "seed",
                SerializationType::PositiveInteger,
                "A dummy seed with generated default = 0.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

// Loads CustomConfig from args.
fn load_custom_config(args: Vec<&str>) -> CustomConfig {
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("config.json");
    CustomConfig { param_path: "default value".to_owned(), seed: 5 }
        .dump_to_file(&vec![], &HashSet::new(), file_path.to_str().unwrap())
        .unwrap();

    load_and_process_config::<CustomConfig>(
        File::open(file_path).unwrap(),
        Command::new("Program"),
        args.into_iter().map(|s| s.to_owned()).collect(),
        false,
    )
    .unwrap()
}

#[test]
fn test_load_default_config() {
    let args = vec!["Testing"];
    let param_path = load_custom_config(args).param_path;
    assert_eq!(param_path, "default value");
}

#[test]
fn test_load_custom_config_file() {
    let args = vec!["Testing", "-f", CUSTOM_CONFIG_PATH.to_str().unwrap()];
    let param_path = load_custom_config(args).param_path;
    assert_eq!(param_path, "custom value");
}

#[test]
fn test_load_custom_config_file_and_args() {
    let args = vec![
        "Testing",
        CONFIG_FILE_ARG,
        CUSTOM_CONFIG_PATH.to_str().unwrap(),
        "--param_path",
        "command value",
    ];
    let param_path = load_custom_config(args).param_path;
    assert_eq!(param_path, "command value");
}

#[test]
fn test_load_many_custom_config_files() {
    let custom_config_path = CUSTOM_CONFIG_PATH.to_str().unwrap();
    let cli_config_param = format!("{custom_config_path},{custom_config_path}");
    let args = vec!["Testing", "-f", cli_config_param.as_str()];
    let param_path = load_custom_config(args).param_path;
    assert_eq!(param_path, "custom value");
}

// Make sure that if we have a field `foo_bar` and an optional field called `foo` with a value of
// None, we don't remove the foo_bar field from the config.
// This test was added following bug #37984 (see bug for more details).
#[test]
fn load_config_allows_optional_fields_can_be_prefixes_of_other_fields() {
    #[derive(Clone, Default, Serialize, Deserialize, Debug, PartialEq)]
    struct ConfigWithOptionalAndPrefixField {
        foo: Option<String>,
        foo_non_optional: String,
    }
    impl SerializeConfig for ConfigWithOptionalAndPrefixField {
        fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
            let mut res = BTreeMap::from([ser_param(
                "foo_non_optional",
                &self.foo_non_optional,
                "This is foo_non_optional.",
                ParamPrivacyInput::Public,
            )]);
            res.extend(ser_optional_param(
                &self.foo,
                "foo".to_string(),
                "foo",
                "This is foo.",
                ParamPrivacyInput::Public,
            ));
            res
        }
    }

    let config_file = NamedTempFile::new().expect("Failed to create test config file");
    let file_path = config_file.path();
    ConfigWithOptionalAndPrefixField {
        foo: None,
        foo_non_optional: "bar non optional".to_string(),
    }
    .dump_to_file(&vec![], &HashSet::new(), file_path.to_str().unwrap())
    .unwrap();

    load_and_process_config::<ConfigWithOptionalAndPrefixField>(
        File::open(file_path).unwrap(),
        Command::new("Program"),
        vec![],
        false,
    )
    .expect("Unexpected error from loading test config.");
}

#[test]
fn test_generated_type() {
    let args = vec!["Testing"];
    assert_eq!(load_custom_config(args).seed, 0);

    let args = vec!["Testing", "--seed", "7"];
    assert_eq!(load_custom_config(args).seed, 7);
}

#[test]
fn serialization_precision() {
    let input =
        "{\"value\":244116128358498188146337218061232635775543270890529169229936851982759783745}";
    let serialized = serde_json::from_str::<serde_json::Value>(input).unwrap();
    let deserialized = serde_json::to_string(&serialized).unwrap();
    assert_eq!(input, deserialized);
}

#[derive(Clone, Default, Serialize, Deserialize, Debug, PartialEq)]
struct RequiredConfig {
    param_path: String,
    num: usize,
}

impl RequiredConfig {
    pub const fn param_path_description() -> &'static str {
        "This is param_path."
    }
    pub const fn num_description() -> &'static str {
        "This is num."
    }
}

impl SerializeConfig for RequiredConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([
            ser_required_param(
                "param_path",
                SerializationType::String,
                Self::param_path_description(),
                ParamPrivacyInput::Public,
            ),
            ser_param("num", &self.num, Self::num_description(), ParamPrivacyInput::Public),
        ])
    }
}

// Loads param_path of RequiredConfig from args.
fn load_required_param_path(args: Vec<&str>) -> String {
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("config.json");
    RequiredConfig { param_path: "default value".to_owned(), num: 3 }
        .dump_to_file(&vec![], &HashSet::new(), file_path.to_str().unwrap())
        .unwrap();

    let loaded_config = load_and_process_config::<CustomConfig>(
        File::open(file_path).unwrap(),
        Command::new("Program"),
        args.into_iter().map(|s| s.to_owned()).collect(),
        false,
    )
    .unwrap();
    loaded_config.param_path
}

#[test]
fn test_negative_required_param() {
    let dumped_config = RequiredConfig { param_path: "0".to_owned(), num: 3 }.dump();
    let (config_map, _) = split_values_and_types(dumped_config);
    let err = load::<RequiredConfig>(&config_map).unwrap_err();
    assert_matches!(err, ConfigError::MissingParam { .. });
}

#[test]
fn test_required_param_from_command() {
    let args = vec!["Testing", "--param_path", "1234"];
    let param_path = load_required_param_path(args);
    assert_eq!(param_path, "1234");
}

#[test]
fn test_required_param_from_file() {
    let args = vec!["Testing", CONFIG_FILE_ARG, CUSTOM_CONFIG_PATH.to_str().unwrap()];
    let param_path = load_required_param_path(args);
    assert_eq!(param_path, "custom value");
}

#[test]
fn deeply_nested_optionals() {
    #[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
    struct Level0 {
        level0_value: u8,
        level1: Option<Level1>,
    }

    impl SerializeConfig for Level0 {
        fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
            let mut res = BTreeMap::from([ser_param(
                "level0_value",
                &self.level0_value,
                "This is level0_value.",
                ParamPrivacyInput::Public,
            )]);
            res.extend(ser_optional_sub_config(&self.level1, "level1"));
            res
        }
    }

    #[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
    struct Level1 {
        pub level1_value: u8,
        pub level2: Option<Level2>,
    }

    impl SerializeConfig for Level1 {
        fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
            let mut res = BTreeMap::from([ser_param(
                "level1_value",
                &self.level1_value,
                "This is level1_value.",
                ParamPrivacyInput::Public,
            )]);
            res.extend(ser_optional_sub_config(&self.level2, "level2"));
            res
        }
    }

    #[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
    struct Level2 {
        pub level2_value: Option<u8>,
    }

    impl SerializeConfig for Level2 {
        fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
            ser_optional_param(
                &self.level2_value,
                1,
                "level2_value",
                "This is level2_value.",
                ParamPrivacyInput::Public,
            )
        }
    }

    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("config2.json");
    Level0 { level0_value: 1, level1: None }
        .dump_to_file(&vec![], &HashSet::new(), file_path.to_str().unwrap())
        .unwrap();

    let l0 = load_and_process_config::<Level0>(
        File::open(file_path.clone()).unwrap(),
        Command::new("Testing"),
        Vec::new(),
        false,
    )
    .unwrap();
    assert_eq!(l0, Level0 { level0_value: 1, level1: None });

    let l1 = load_and_process_config::<Level0>(
        File::open(file_path.clone()).unwrap(),
        Command::new("Testing"),
        vec!["Testing".to_owned(), "--level1.#is_none".to_owned(), "false".to_owned()],
        false,
    )
    .unwrap();
    assert_eq!(
        l1,
        Level0 { level0_value: 1, level1: Some(Level1 { level1_value: 0, level2: None }) }
    );

    let l2 = load_and_process_config::<Level0>(
        File::open(file_path.clone()).unwrap(),
        Command::new("Testing"),
        vec![
            "Testing".to_owned(),
            "--level1.#is_none".to_owned(),
            "false".to_owned(),
            "--level1.level2.#is_none".to_owned(),
            "false".to_owned(),
        ],
        false,
    )
    .unwrap();
    assert_eq!(
        l2,
        Level0 {
            level0_value: 1,
            level1: Some(Level1 { level1_value: 0, level2: Some(Level2 { level2_value: None }) }),
        }
    );

    let l2_value = load_and_process_config::<Level0>(
        File::open(file_path).unwrap(),
        Command::new("Testing"),
        vec![
            "Testing".to_owned(),
            "--level1.#is_none".to_owned(),
            "false".to_owned(),
            "--level1.level2.#is_none".to_owned(),
            "false".to_owned(),
            "--level1.level2.level2_value.#is_none".to_owned(),
            "false".to_owned(),
        ],
        false,
    )
    .unwrap();
    assert_eq!(
        l2_value,
        Level0 {
            level0_value: 1,
            level1: Some(Level1 {
                level1_value: 0,
                level2: Some(Level2 { level2_value: Some(1) }),
            }),
        }
    );
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
struct TestConfigWithNestedJson {
    #[serde(deserialize_with = "deserialize_optional_list_with_nested_json")]
    list: Option<Vec<BTreeMap<String, HashMap<String, u64>>>>,
}

impl SerializeConfig for TestConfigWithNestedJson {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([ser_param(
            "list",
            &serialize_optional_list_with_nested_json(&self.list),
            "A list of nested JSON values.",
            ParamPrivacyInput::Public,
        )])
    }
}
#[test]
fn optional_list_nested_json() {
    let config = TestConfigWithNestedJson {
        list: Some(vec![BTreeMap::from([("a".to_owned(), HashMap::from([("b".to_owned(), 1)]))])]),
    };
    let dumped = config.dump();
    let (config_map, _) = split_values_and_types(dumped);
    let loaded_config = load::<TestConfigWithNestedJson>(&config_map).unwrap();
    assert_eq!(loaded_config.list, config.list);
    let serialized = serde_json::to_string(&loaded_config).unwrap();
    assert_eq!(serialized, r#"{"list":[{"a":{"b":1}}]}"#);
}
