//! Utils for serialization and deserialization of nested config fields into simple types.
//! These conversions let the command line updater (which supports only numbers strings and
//! booleans) handle these fields.
//!
//! # example
//!
//! ```
//! use std::collections::BTreeMap;
//! use std::time::Duration;
//!
//! use apollo_config::converters::deserialize_milliseconds_to_duration;
//! use apollo_config::loading::load;
//! use serde::Deserialize;
//! use serde_json::json;
//!
//! #[derive(Clone, Deserialize, Debug, PartialEq)]
//! struct DurationConfig {
//!     #[serde(deserialize_with = "deserialize_milliseconds_to_duration")]
//!     dur: Duration,
//! }
//!
//! let dumped_config = BTreeMap::from([("dur".to_owned(), json!(1000))]);
//! let loaded_config = load::<DurationConfig>(&dumped_config).unwrap();
//! assert_eq!(loaded_config.dur.as_secs(), 1);
//! ```

use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};
use url::Url;

/// Deserializes milliseconds to duration object.
pub fn deserialize_milliseconds_to_duration<'de, D>(de: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    let millis: u64 = Deserialize::deserialize(de)?;
    Ok(Duration::from_millis(millis))
}

/// Deserializes seconds to duration object.
pub fn deserialize_seconds_to_duration<'de, D>(de: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    let secs: u64 = Deserialize::deserialize(de)?;
    Ok(Duration::from_secs(secs))
}

/// Deserializes float seconds to duration object.
pub fn deserialize_float_seconds_to_duration<'de, D>(de: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    let secs: f64 = Deserialize::deserialize(de)?;
    Ok(Duration::from_secs_f64(secs))
}

/// Serializes a map to "k1:v1 k2:v2" string structure.
pub fn serialize_optional_map(optional_map: &Option<HashMap<String, String>>) -> String {
    match optional_map {
        None => "".to_owned(),
        Some(map) => map.iter().map(|(k, v)| format!("{k}:{v}")).collect::<Vec<String>>().join(" "),
    }
}

/// Deserializes a map from "k1:v1 k2:v2" string structure.
pub fn deserialize_optional_map<'de, D>(de: D) -> Result<Option<HashMap<String, String>>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw_str: String = Deserialize::deserialize(de)?;
    if raw_str.is_empty() {
        return Ok(None);
    }

    let mut map = HashMap::new();
    for raw_pair in raw_str.split(' ') {
        let split: Vec<&str> = raw_pair.split(':').collect();
        if split.len() != 2 {
            return Err(D::Error::custom(format!(
                "pair \"{raw_pair}\" is not valid. The Expected format is name:value"
            )));
        }
        map.insert(split[0].to_string(), split[1].to_string());
    }
    Ok(Some(map))
}

/// A struct containing a URL and its associated headers.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct UrlAndHeaders {
    /// The base URL.
    pub url: Url,
    /// A map of header keyword-value pairs.
    pub headers: BTreeMap<String, String>,
}

/// Serializes a vector containing nested BTreeMaps into a space-separated string.
pub fn serialize_optional_list_with_url_and_headers(list: &Option<Vec<UrlAndHeaders>>) -> String {
    match list {
        None => "".to_owned(),
        Some(list) => list
            .iter()
            .map(|item| {
                serde_json::to_string(item).expect("Failed to serialize UrlAndHeader to JSON")
            })
            .collect::<Vec<String>>()
            .join(" "),
    }
}

/// Deserializes a space-separated string into a vector of nested BTreeMaps.
/// Returns an error if any of the substrings cannot be parsed into a valid structs.
pub fn deserialize_optional_list_with_url_and_headers<'de, D>(
    de: D,
) -> Result<Option<Vec<UrlAndHeaders>>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw: String = Deserialize::deserialize(de)?;
    if raw.trim().is_empty() {
        return Ok(None);
    }
    let number_of_items = raw.split_whitespace().count();
    let mut output = Vec::with_capacity(number_of_items);
    for item in raw.split_whitespace() {
        let value: UrlAndHeaders = serde_json::from_str(item)
            .map_err(|e| D::Error::custom(format!("Invalid JSON '{}': {}", item, e)))?;
        output.push(value);
    }
    Ok(Some(output))
}

/// Serializes a vector to string structure. The vector is expected to be a hex string.
pub fn serialize_optional_vec_u8(optional_vector: &Option<Vec<u8>>) -> String {
    match optional_vector {
        None => "".to_owned(),
        Some(vector) => {
            format!(
                "0x{}",
                vector.iter().map(|num| format!("{:02x}", num)).collect::<Vec<String>>().join("")
            )
        }
    }
}

/// Deserializes a vector from string structure. The vector is expected to be a list of u8 values
/// separated by spaces.
pub fn deserialize_optional_vec_u8<'de, D>(de: D) -> Result<Option<Vec<u8>>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw_str: String = Deserialize::deserialize(de)?;
    if raw_str.is_empty() {
        return Ok(None);
    }

    if !raw_str.starts_with("0x") {
        return Err(D::Error::custom(
            "Couldn't deserialize vector. Expected hex string starting with \"0x\"",
        ));
    }

    let hex_str = &raw_str[2..]; // Strip the "0x" prefix

    let mut vector = Vec::new();
    for i in (0..hex_str.len()).step_by(2) {
        let byte_str = &hex_str[i..i + 2];
        let byte = u8::from_str_radix(byte_str, 16).map_err(|e| {
            D::Error::custom(format!(
                "Couldn't deserialize vector. Failed to parse byte: {} {}",
                byte_str, e
            ))
        })?;
        vector.push(byte);
    }
    Ok(Some(vector))
}

// TODO(Tsabary): generalize these for Vec<T> serde.

/// Serializes a `&[Url]` into a single space-separated string.
pub fn serialize_slice_url(vector: &[Url]) -> String {
    vector.iter().map(Url::as_str).collect::<Vec<_>>().join(" ")
}

/// Deserializes a space-separated string into a `Vec<Url>`.
/// Returns an error if any of the substrings cannot be parsed into a valid URL.
pub fn deserialize_vec_url<'de, D>(de: D) -> Result<Vec<Url>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw: String = <String as serde::Deserialize>::deserialize(de)?;

    if raw.trim().is_empty() {
        return Ok(Vec::new());
    }

    raw.split_whitespace()
        .map(|s| Url::parse(s).map_err(|e| D::Error::custom(format!("Invalid URL '{}': {}", s, e))))
        .collect()
}
