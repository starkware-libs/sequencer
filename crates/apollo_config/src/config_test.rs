use std::collections::BTreeMap;
use std::io::Write;

use clap::Command;
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;
use url::Url;

use crate::converters::{
    deserialize_optional_list_with_url_and_headers,
    serialize_optional_list_with_url_and_headers,
    UrlAndHeaders,
};
use crate::loading::load_and_process_config;
use crate::presentation::get_config_presentation;

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
struct SimpleConfig {
    value: u32,
    name: String,
    optional: Option<u32>,
}

#[test]
fn load_from_nested_json_file() {
    let config = SimpleConfig { value: 42, name: "test".to_owned(), optional: Some(7) };
    let mut tmp = NamedTempFile::new().unwrap();
    write!(tmp, "{}", serde_json::to_string(&config).unwrap()).unwrap();
    let path = tmp.path().to_str().unwrap().to_owned();

    let loaded: SimpleConfig =
        load_and_process_config(Command::new("t"), vec!["t".into(), "--config_file".into(), path])
            .unwrap();
    assert_eq!(loaded, config);
}

#[test]
fn load_null_optional_from_nested_json_file() {
    let config = SimpleConfig { value: 1, name: "x".to_owned(), optional: None };
    let mut tmp = NamedTempFile::new().unwrap();
    write!(tmp, "{}", serde_json::to_string(&config).unwrap()).unwrap();
    let path = tmp.path().to_str().unwrap().to_owned();

    let loaded: SimpleConfig =
        load_and_process_config(Command::new("t"), vec!["t".into(), "--config_file".into(), path])
            .unwrap();
    assert_eq!(loaded, config);
}

// --------- Presentation tests ---------

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
struct TypicalConfig {
    a: u32,
    b: String,
    c: bool,
}

#[derive(Clone, Default, Serialize, Deserialize, Debug, PartialEq)]
struct InnerConfig {
    o: usize,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
struct OuterConfig {
    opt_elem: Option<usize>,
    inner_config: InnerConfig,
}

#[test]
fn test_config_presentation_public_vs_private() {
    let config = TypicalConfig { a: 1, b: "bbb".to_owned(), c: false };
    let presentation = get_config_presentation(&config, &["c"], true).unwrap();
    let all_keys: Vec<_> = presentation.as_object().unwrap().keys().collect();
    assert_eq!(all_keys, vec!["a", "b", "c"]);

    let public_presentation = get_config_presentation(&config, &["c"], false).unwrap();
    let public_keys: Vec<_> = public_presentation.as_object().unwrap().keys().collect();
    assert_eq!(public_keys, vec!["a", "b"]);
}

#[test]
fn test_nested_config_presentation() {
    let config = OuterConfig { opt_elem: Some(1), inner_config: InnerConfig { o: 3 } };
    let presentation = get_config_presentation(&config, &[], true).unwrap();
    let keys: Vec<_> = presentation.as_object().unwrap().keys().collect();
    assert_eq!(keys, vec!["inner_config", "opt_elem"]);
}

// --------- Converter tests ---------

#[test]
fn url_and_headers_converter_roundtrip() {
    let original = Some(vec![
        UrlAndHeaders {
            url: Url::parse("http://a.com/").unwrap(),
            headers: BTreeMap::from([
                ("key1".to_owned(), "value 1".to_owned()),
                ("key2".to_owned(), "value 2".to_owned()),
            ]),
        },
        UrlAndHeaders { url: Url::parse("http://b.com/").unwrap(), headers: BTreeMap::new() },
    ]);
    let serialized = serialize_optional_list_with_url_and_headers(&original);
    assert_eq!(serialized, "http://a.com/,key1^value 1,key2^value 2|http://b.com/");

    // Round-trip through deserializer.
    #[derive(Deserialize)]
    struct W {
        #[serde(deserialize_with = "deserialize_optional_list_with_url_and_headers")]
        list: Option<Vec<UrlAndHeaders>>,
    }
    let w: W = serde_json::from_value(serde_json::json!({ "list": serialized })).unwrap();
    assert_eq!(w.list, original);
}
