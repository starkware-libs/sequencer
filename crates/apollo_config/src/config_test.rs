use std::collections::{BTreeMap, BTreeSet};
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
use crate::loading::{load, load_and_process_config};
use crate::presentation::get_config_presentation;
use crate::{ConfigError, ParamPath, CONFIG_FILE_ARG};

#[derive(Clone, Copy, Default, Serialize, Deserialize, Debug, PartialEq, Validate)]
struct InnerConfig {
    #[validate(range(min = 0, max = 10))]
    o: usize,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Validate)]
struct OuterConfig {
    opt_elem: Option<usize>,
    opt_config: Option<InnerConfig>,
    #[validate(nested)]
    inner_config: InnerConfig,
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
    // `c` is the secret param for this fixture; production callers inject the private path set
    // (e.g. `private_parameters()`) rather than deriving it.
    let private_paths = BTreeSet::from(["c".to_owned()]);

    let presentation = get_config_presentation(&config, true, &private_paths).unwrap();
    let keys: Vec<_> = presentation.as_object().unwrap().keys().collect();
    assert_eq!(keys, vec!["a", "b", "c", "d", "e", "f"]);

    let public_presentation = get_config_presentation(&config, false, &private_paths).unwrap();
    let keys: Vec<_> = public_presentation.as_object().unwrap().keys().collect();
    assert_eq!(keys, vec!["a", "b", "d", "e", "f"]);
}

#[test]
fn test_config_presentation_does_not_leak_private_params() {
    let config = TypicalConfig {
        a: Duration::from_secs(1),
        b: "bbb".to_owned(),
        c: true,
        d: -1,
        e: 10,
        f: 0.5,
    };
    // Inject `c` as the private path (the secret), matching what production callers do.
    let private_paths = BTreeSet::from(["c".to_owned()]);

    // Redacted presentation must NOT contain the private param.
    let public_presentation = get_config_presentation(&config, false, &private_paths).unwrap();
    assert!(
        public_presentation.as_object().unwrap().get("c").is_none(),
        "private param `c` leaked into the redacted presentation"
    );

    // Full presentation MUST contain it.
    let full_presentation = get_config_presentation(&config, true, &private_paths).unwrap();
    assert!(full_presentation.as_object().unwrap().get("c").is_some());
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

    // This fixture declares no private params, so nothing is redacted.
    let private_paths = BTreeSet::new();

    for config in configs {
        let presentation = get_config_presentation(&config, true, &private_paths).unwrap();
        let keys: Vec<_> = presentation.as_object().unwrap().keys().collect();
        assert_eq!(keys, vec!["inner_config", "opt_config", "opt_elem"]);
        let public_presentation = get_config_presentation(&config, false, &private_paths).unwrap();
        let keys: Vec<_> = public_presentation.as_object().unwrap().keys().collect();
        assert_eq!(keys, vec!["inner_config", "opt_config", "opt_elem"]);
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

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
struct TestConfigWithNestedJson {
    #[serde(deserialize_with = "deserialize_optional_list_with_url_and_headers")]
    list_of_maps: Option<Vec<UrlAndHeaders>>,
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
    // Build the flat single-leaf values map directly from the converter, then load it back.
    let config_map: BTreeMap<ParamPath, serde_json::Value> = BTreeMap::from([(
        "list_of_maps".to_owned(),
        json!(serialize_optional_list_with_url_and_headers(&config.list_of_maps)),
    )]);
    let loaded_config = load::<TestConfigWithNestedJson>(&config_map).unwrap();
    assert_eq!(loaded_config.list_of_maps, config.list_of_maps);
    let serialized = serde_json::to_string(&config_map).unwrap();
    assert_eq!(
        serialized,
        r#"{"list_of_maps":"http://a.com/,key1^value 1,key2^value 2|http://b.com/,key3^value 3,key4^value 4|http://c.com/|http://d.com/,key5^value 5"}"#
    );
}
