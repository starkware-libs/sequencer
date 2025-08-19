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
use std::str::FromStr;
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

impl UrlAndHeaders {
    /// Reserved characters that are not allowed in keys or values.
    const RESERVED_CHARS: [char; 2] = ['^', ','];

    /// Serialize into: url,key1^val1,key2^val2,...
    pub fn to_custom_string(&self) -> Result<String, String> {
        for (k, v) in &self.headers {
            Self::validate_component(k, "key")?;
            Self::validate_component(v, "value")?;
        }

        let mut output = self.url.as_str().to_string();
        for (key, value) in &self.headers {
            output.push(',');
            output.push_str(key);
            output.push('^');
            output.push_str(value);
        }
        Ok(output)
    }

    /// Deserialize from: url,key1^val1,key2^val2,...
    pub fn from_custom_string(s: &str) -> Result<Self, String> {
        // Split the string into URL and headers on the first comma.
        let mut parts = s.splitn(2, ',');
        let url_str = parts.next().ok_or("Missing URL")?;
        let rest = parts.next().unwrap_or("");

        let url = Url::parse(url_str).map_err(|e| format!("Invalid URL: {}", e))?;

        let mut headers = BTreeMap::new();
        if !rest.is_empty() {
            for pair in rest.split(',') {
                let mut kv = pair.splitn(2, '^');
                let k = kv.next().ok_or("Missing header key")?;
                let v = kv.next().ok_or("Missing header value")?;

                Self::validate_component(k, "key")?;
                Self::validate_component(v, "value")?;

                headers.insert(k.to_string(), v.to_string());
            }
        }

        Ok(UrlAndHeaders { url, headers })
    }

    fn validate_component(value: &str, label: &str) -> Result<(), String> {
        if let Some(c) = value.chars().find(|c| Self::RESERVED_CHARS.contains(c)) {
            return Err(format!("Invalid character '{}' in header {}: '{}'", c, label, value));
        }
        Ok(())
    }
}

/// Serializes a vector containing the UrlAndHeaders struct into a space-separated string.
pub fn serialize_optional_list_with_url_and_headers(list: &Option<Vec<UrlAndHeaders>>) -> String {
    match list {
        None => "".to_owned(),
        Some(list) => list
            .iter()
            .map(|item| {
                UrlAndHeaders::to_custom_string(item).expect("Failed to serialize UrlAndHeader")
            })
            .collect::<Vec<String>>()
            .join("|"),
    }
}

/// Deserializes a space-separated string into a vector of UrlAndHeaders structs.
/// Returns an error if any of the substrings cannot be parsed into a valid struct.
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
    let items = raw.split('|');
    let number_of_items = items.clone().count();
    let mut output = Vec::with_capacity(number_of_items);
    for item in items {
        let value: UrlAndHeaders = UrlAndHeaders::from_custom_string(item).map_err(|e| {
            D::Error::custom(format!("Invalid UrlAndHeaders formatting '{}': {}", item, e))
        })?;
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

/// Serializes a `&[Url]` into a single space-separated string.
pub fn serialize_slice<T: AsRef<str>>(vector: &[T]) -> String {
    vector.iter().map(|item| item.as_ref()).collect::<Vec<_>>().join(" ")
}

/// Deserializes a space-separated string into a `Vec<T>`.
/// Returns an error if any of the substrings cannot be parsed into `T`.
pub fn deserialize_vec<'de, D, T>(de: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr,
    T::Err: std::fmt::Display,
{
    let raw: String = <String as serde::Deserialize>::deserialize(de)?;

    if raw.trim().is_empty() {
        return Ok(Vec::new());
    }

    raw.split_whitespace()
        .map(|s| {
            T::from_str(s).map_err(|e| D::Error::custom(format!("Invalid value '{}': {}", s, e)))
        })
        .collect()
}
