use std::collections::{BTreeMap, HashSet};
use std::time::Duration;

use assert_matches::assert_matches;
use clap::Command;
use itertools::chain;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tempfile::NamedTempFile;
use url::Url;
use validator::Validate;

use crate::converters::{
    deserialize_milliseconds_to_duration,
    deserialize_optional_list_with_url_and_headers,
    serialize_optional_list_with_url_and_headers,
    UrlAndHeaders,
};
use crate::dumping::{
    combine_config_map_and_pointers,
    generate_struct_pointer,
    prepend_sub_config_name,
    required_param_description,
    ser_optional_param,
    ser_optional_sub_config,
    ser_param,
    ser_pointer_target_param,
    ser_pointer_target_required_param,
    ser_required_param,
    set_pointing_param_paths,
    SerializeConfig,
};
use crate::loading::{load, load_and_process_config};
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
    #[validate(nested)]
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

#[derive(Deserialize, Debug, PartialEq)]
struct NativeInnerConfig {
    a: usize,
    b: String,
}

#[derive(Deserialize, Debug, PartialEq)]
struct NativeConfig {
    top: String,
    inner: NativeInnerConfig,
}

// Mirrors the real secrets layout: dotted keys reach deep, real, nested leaves (e.g.
// `consensus_manager_config.network_config.secret_key`), never single-token pointer targets.
#[derive(Deserialize, Debug, PartialEq)]
struct NativeNetworkConfig {
    secret_key: String,
    other_network_field: bool,
}

#[derive(Deserialize, Debug, PartialEq)]
struct NativeComponentConfig {
    network_config: NativeNetworkConfig,
    other_component_field: u32,
}

#[derive(Deserialize, Debug, PartialEq)]
struct NativeDeepConfig {
    component_config: NativeComponentConfig,
    other_top_field: String,
}

// A `None` optional sub-config, mirroring a disabled component (e.g. `central_sync_client_config`).
#[derive(Deserialize, Debug, PartialEq)]
struct NativeOptionalConfig {
    optional_component: Option<NativeComponentConfig>,
    top_field: String,
}

// Loads a config via the native loader: the first `--config_file` is the nested base, each
// subsequent one a flat dotted-key secret override.
fn load_native_config<T: for<'a> Deserialize<'a>>(
    config_file_args: Vec<&str>,
) -> Result<T, ConfigError> {
    let args = chain!(["Testing"], config_file_args).map(|arg| arg.to_owned()).collect();
    load_and_process_config::<T>(Command::new("Program"), args, false)
}

#[test]
fn test_native_config_with_secret_overrides() {
    let base_file = NamedTempFile::new().unwrap();
    let base_config = json!({"top": "base_top", "inner": {"a": 1, "b": "base_b"}});
    std::fs::write(base_file.path(), base_config.to_string()).unwrap();
    // Secrets stay flat dotted-key and are applied onto the nested base.
    let secret_file = NamedTempFile::new().unwrap();
    std::fs::write(secret_file.path(), json!({"inner.b": "secret_b"}).to_string()).unwrap();

    let loaded: NativeConfig = load_native_config(vec![
        CONFIG_FILE_ARG,
        base_file.path().to_str().unwrap(),
        CONFIG_FILE_ARG,
        secret_file.path().to_str().unwrap(),
    ])
    .unwrap();

    assert_eq!(
        loaded,
        NativeConfig {
            top: "base_top".to_owned(),
            inner: NativeInnerConfig { a: 1, b: "secret_b".to_owned() },
        }
    );
}

#[test]
fn test_native_config_with_empty_secrets() {
    let base_file = NamedTempFile::new().unwrap();
    let base_config = json!({"top": "base_top", "inner": {"a": 1, "b": "base_b"}});
    std::fs::write(base_file.path(), base_config.to_string()).unwrap();
    // Native mode requires exactly two files; an empty secrets file leaves the base config intact.
    let secret_file = NamedTempFile::new().unwrap();
    std::fs::write(secret_file.path(), json!({}).to_string()).unwrap();

    let loaded: NativeConfig = load_native_config(vec![
        CONFIG_FILE_ARG,
        base_file.path().to_str().unwrap(),
        CONFIG_FILE_ARG,
        secret_file.path().to_str().unwrap(),
    ])
    .unwrap();

    assert_eq!(
        loaded,
        NativeConfig {
            top: "base_top".to_owned(),
            inner: NativeInnerConfig { a: 1, b: "base_b".to_owned() },
        }
    );
}

#[test]
fn test_native_config_requires_two_config_files() {
    let base_file = NamedTempFile::new().unwrap();
    let base_config = json!({"top": "base_top", "inner": {"a": 1, "b": "base_b"}});
    std::fs::write(base_file.path(), base_config.to_string()).unwrap();

    // A single config file is rejected: the native loader requires a base and a secrets file.
    let result = load_native_config::<NativeConfig>(vec![
        CONFIG_FILE_ARG,
        base_file.path().to_str().unwrap(),
    ]);
    assert_matches!(result, Err(ConfigError::NativeModeRequiresTwoConfigFiles));
}

#[test]
fn test_native_config_without_config_file() {
    let result = load_native_config::<NativeConfig>(vec![]);
    assert_matches!(result, Err(ConfigError::NativeModeRequiresTwoConfigFiles));
}

// Mirrors the production secrets file (crates/apollo_deployments/resources/testing_secrets.json):
// each secret is a flat dotted key naming a deep, real nested leaf. Asserts the override lands at
// exactly that leaf and every sibling at every level is preserved.
#[test]
fn test_native_config_deep_secret_override_preserves_siblings() {
    let base_file = NamedTempFile::new().unwrap();
    let base_config = json!({
        "other_top_field": "base_top",
        "component_config": {
            "other_component_field": 7,
            "network_config": {
                "secret_key": "base_secret",
                "other_network_field": true
            }
        }
    });
    std::fs::write(base_file.path(), base_config.to_string()).unwrap();

    let secret_file = NamedTempFile::new().unwrap();
    let secret_config = json!({"component_config.network_config.secret_key": "0xabc"});
    std::fs::write(secret_file.path(), secret_config.to_string()).unwrap();

    let loaded: NativeDeepConfig = load_native_config(vec![
        CONFIG_FILE_ARG,
        base_file.path().to_str().unwrap(),
        CONFIG_FILE_ARG,
        secret_file.path().to_str().unwrap(),
    ])
    .unwrap();

    assert_eq!(
        loaded,
        NativeDeepConfig {
            other_top_field: "base_top".to_owned(),
            component_config: NativeComponentConfig {
                other_component_field: 7,
                network_config: NativeNetworkConfig {
                    secret_key: "0xabc".to_owned(),
                    other_network_field: true,
                },
            },
        }
    );
}

// A secret aimed at a child of a `None` (null) intermediate must be skipped, not vivified into a
// partial object — otherwise deserialization of the (incomplete) sub-config would fail. The
// disabled component stays `None`.
#[test]
fn test_native_config_skips_secret_under_none_intermediate() {
    let base_file = NamedTempFile::new().unwrap();
    let base_config = json!({"optional_component": null, "top_field": "base"});
    std::fs::write(base_file.path(), base_config.to_string()).unwrap();

    let secret_file = NamedTempFile::new().unwrap();
    let secret_config = json!({"optional_component.network_config.secret_key": "0xabc"});
    std::fs::write(secret_file.path(), secret_config.to_string()).unwrap();

    let loaded: NativeOptionalConfig = load_native_config(vec![
        CONFIG_FILE_ARG,
        base_file.path().to_str().unwrap(),
        CONFIG_FILE_ARG,
        secret_file.path().to_str().unwrap(),
    ])
    .unwrap();

    assert_eq!(
        loaded,
        NativeOptionalConfig { optional_component: None, top_field: "base".to_owned() }
    );
}

// A secret naming a leaf that is absent from an existing parent object is created (the parent
// exists, so the component is enabled and the override is relevant).
#[test]
fn test_native_config_creates_absent_leaf_in_existing_parent() {
    let base_file = NamedTempFile::new().unwrap();
    let base_config = json!({"top": "base_top", "inner": {"a": 1}});
    std::fs::write(base_file.path(), base_config.to_string()).unwrap();

    let secret_file = NamedTempFile::new().unwrap();
    std::fs::write(secret_file.path(), json!({"inner.b": "created_b"}).to_string()).unwrap();

    let loaded: NativeConfig = load_native_config(vec![
        CONFIG_FILE_ARG,
        base_file.path().to_str().unwrap(),
        CONFIG_FILE_ARG,
        secret_file.path().to_str().unwrap(),
    ])
    .unwrap();

    assert_eq!(
        loaded,
        NativeConfig {
            top: "base_top".to_owned(),
            inner: NativeInnerConfig { a: 1, b: "created_b".to_owned() },
        }
    );
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

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
struct TestConfigWithNestedJson {
    #[serde(deserialize_with = "deserialize_optional_list_with_url_and_headers")]
    list_of_maps: Option<Vec<UrlAndHeaders>>,
}

impl SerializeConfig for TestConfigWithNestedJson {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([ser_param(
            "list_of_maps",
            &serialize_optional_list_with_url_and_headers(&self.list_of_maps),
            "A list of nested JSON values.",
            ParamPrivacyInput::Public,
        )])
    }
}

#[test]
fn optional_list_nested_btreemaps() {
    let config = TestConfigWithNestedJson {
        list_of_maps: Some(vec![
            UrlAndHeaders {
                url: Url::parse("http://a.com/").unwrap(),
                headers: BTreeMap::from([
                    ("key1".to_owned(), "value 1".to_owned()),
                    ("key2".to_owned(), "value 2".to_owned()),
                ]),
            },
            UrlAndHeaders {
                url: Url::parse("http://b.com/").unwrap(),
                headers: BTreeMap::from([
                    ("key3".to_owned(), "value 3".to_owned()),
                    ("key4".to_owned(), "value 4".to_owned()),
                ]),
            },
            UrlAndHeaders {
                url: Url::parse("http://c.com/").unwrap(),
                headers: BTreeMap::from([]),
            },
            UrlAndHeaders {
                url: Url::parse("http://d.com/").unwrap(),
                headers: BTreeMap::from([("key5".to_owned(), "value 5".to_owned())]),
            },
        ]),
    };
    // Build the flat values map directly from the dump (a single serialized leaf), then load it.
    let config_map: BTreeMap<ParamPath, serde_json::Value> = config
        .dump()
        .into_iter()
        .filter_map(|(param_path, serialized_param)| match serialized_param.content {
            SerializedContent::DefaultValue(value) => Some((param_path, value)),
            _ => None,
        })
        .collect();
    let loaded_config = load::<TestConfigWithNestedJson>(&config_map).unwrap();
    assert_eq!(loaded_config.list_of_maps, config.list_of_maps);
    let serialized = serde_json::to_string(&config_map).unwrap();
    assert_eq!(
        serialized,
        r#"{"list_of_maps":"http://a.com/,key1^value 1,key2^value 2|http://b.com/,key3^value 3,key4^value 4|http://c.com/|http://d.com/,key5^value 5"}"#
    );
}
