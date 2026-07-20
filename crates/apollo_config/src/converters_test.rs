use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{
    deserialize_float_seconds_to_duration,
    deserialize_milliseconds_to_duration,
    deserialize_optional_list_with_url_and_headers,
    deserialize_optional_map,
    deserialize_optional_vec_u8,
    deserialize_seconds_to_duration,
    serialize_duration_as_float_seconds,
    serialize_duration_as_milliseconds,
    serialize_duration_as_seconds,
    UrlAndHeaders,
};

// These wrappers mirror the `#[serde(deserialize_with = ..., serialize_with = ...)]` pairings used
// on real config fields. Asserting `from_value(to_value(x)) == x` proves the serializer emits
// exactly the wire shape the deserializer reads, so `serde_json::to_value(config)` (used by the
// native-config harness and the startup-log presentation) round-trips.

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct MillisWrapper {
    #[serde(
        deserialize_with = "deserialize_milliseconds_to_duration",
        serialize_with = "serialize_duration_as_milliseconds"
    )]
    duration: Duration,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct SecondsWrapper {
    #[serde(
        deserialize_with = "deserialize_seconds_to_duration",
        serialize_with = "serialize_duration_as_seconds"
    )]
    duration: Duration,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct FloatSecondsWrapper {
    #[serde(
        deserialize_with = "deserialize_float_seconds_to_duration",
        serialize_with = "serialize_duration_as_float_seconds"
    )]
    duration: Duration,
}

fn assert_round_trips<T>(value: T)
where
    T: Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let serialized = serde_json::to_value(&value).unwrap();
    let deserialized: T = serde_json::from_value(serialized).unwrap();
    assert_eq!(value, deserialized);
}

#[test]
fn milliseconds_duration_round_trips() {
    // Default (zero) and a non-default value.
    assert_round_trips(MillisWrapper { duration: Duration::ZERO });
    assert_round_trips(MillisWrapper { duration: Duration::from_millis(1234) });
}

#[test]
fn seconds_duration_round_trips() {
    assert_round_trips(SecondsWrapper { duration: Duration::ZERO });
    assert_round_trips(SecondsWrapper { duration: Duration::from_secs(987) });
}

#[test]
fn float_seconds_duration_round_trips() {
    assert_round_trips(FloatSecondsWrapper { duration: Duration::ZERO });
    // `deserialize_float_seconds_to_duration` uses `from_secs_f64`, so pick a value that is exactly
    // representable to avoid float rounding noise in the equality assertion.
    assert_round_trips(FloatSecondsWrapper { duration: Duration::from_secs_f64(1.5) });
}

#[derive(Debug, PartialEq, Deserialize)]
struct OptionalStringEncodedWrapper {
    #[serde(deserialize_with = "deserialize_optional_map")]
    map: Option<HashMap<String, String>>,
    #[serde(deserialize_with = "deserialize_optional_vec_u8")]
    bytes: Option<Vec<u8>>,
    #[serde(deserialize_with = "deserialize_optional_list_with_url_and_headers")]
    urls: Option<Vec<UrlAndHeaders>>,
}

/// A `None` field serialized by derived `Serialize` (`null`) loads back as `None`, like the empty
/// string the secrets files use.
#[test]
fn optional_string_encoded_fields_accept_null_and_empty() {
    for none_form in [json!(null), json!("")] {
        let wrapper: OptionalStringEncodedWrapper = serde_json::from_value(json!({
            "map": none_form,
            "bytes": none_form,
            "urls": none_form,
        }))
        .unwrap();
        assert_eq!(
            wrapper,
            OptionalStringEncodedWrapper { map: None, bytes: None, urls: None },
            "none form {none_form:?}"
        );
    }
    let wrapper: OptionalStringEncodedWrapper = serde_json::from_value(json!({
        "map": "a:1 b:2",
        "bytes": "0x0a0b",
        "urls": null,
    }))
    .unwrap();
    assert_eq!(
        wrapper.map,
        Some(HashMap::from([("a".to_owned(), "1".to_owned()), ("b".to_owned(), "2".to_owned())]))
    );
    assert_eq!(wrapper.bytes, Some(vec![0x0a, 0x0b]));
}
