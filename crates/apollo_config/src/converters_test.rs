use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::{
    deserialize_comma_separated_str,
    deserialize_float_seconds_to_duration,
    deserialize_milliseconds_to_duration,
    deserialize_seconds_to_duration,
    serialize_duration_as_float_seconds,
    serialize_duration_as_milliseconds,
    serialize_duration_as_seconds,
    serialize_optional_comma_separated_str,
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

// `deserialize_comma_separated_str`/`serialize_optional_comma_separated_str` are generic over any
// `T: FromStr + ToString`; `u64` exercises the same code path without pulling in `starknet_api`.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct CommaSeparatedWrapper {
    #[serde(
        deserialize_with = "deserialize_comma_separated_str",
        serialize_with = "serialize_optional_comma_separated_str"
    )]
    list: Option<Vec<u64>>,
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

#[test]
fn comma_separated_list_round_trips() {
    // `None` (the default) and a populated list.
    assert_round_trips(CommaSeparatedWrapper { list: None });
    assert_round_trips(CommaSeparatedWrapper { list: Some(vec![1, 22, 333]) });
}
