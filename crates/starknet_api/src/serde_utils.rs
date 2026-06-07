//! Utilities for serialising/deserialising values.
#[cfg(test)]
#[path = "serde_utils_test.rs"]
mod serde_utils_test;

use serde::de::{Deserialize, Visitor};
use serde::ser::{Serialize, SerializeTuple};
use serde::Deserializer;
use serde_json::Value;

use crate::deprecated_contract_class::ContractClassAbiEntry;
use crate::transaction::{
    DeclareTransaction,
    DeployAccountTransaction,
    InvokeTransaction,
    Transaction,
};

/// A [BytesAsHex](`crate::serde_utils::BytesAsHex`) prefixed with '0x'.
pub type PrefixedBytesAsHex<const N: usize> = BytesAsHex<N, true>;

/// A byte array that serializes as a hex string.
///
/// The `PREFIXED` generic type symbolize whether a string representation of the hex value should be
/// prefixed by `0x` or not.
#[derive(Debug, Eq, PartialEq)]
pub struct BytesAsHex<const N: usize, const PREFIXED: bool>(pub(crate) [u8; N]);

impl<'de, const N: usize, const PREFIXED: bool> Deserialize<'de> for BytesAsHex<N, PREFIXED> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ByteArrayVisitor<const N: usize, const PREFIXED: bool>;
        impl<'de, const N: usize, const PREFIXED: bool> Visitor<'de> for ByteArrayVisitor<N, PREFIXED> {
            type Value = BytesAsHex<N, PREFIXED>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a byte array")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut res = [0u8; N];
                let mut i = 0;
                while let Some(value) = seq.next_element()? {
                    res[i] = value;
                    i += 1;
                }
                Ok(BytesAsHex(res))
            }
        }

        if deserializer.is_human_readable() {
            let s = String::deserialize(deserializer)?;
            bytes_from_hex_str::<N, PREFIXED>(s.as_str())
                .map_err(serde::de::Error::custom)
                .map(BytesAsHex)
        } else {
            deserializer.deserialize_tuple(N, ByteArrayVisitor)
        }
    }
}

impl<const N: usize, const PREFIXED: bool> Serialize for BytesAsHex<N, PREFIXED> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if serializer.is_human_readable() {
            let hex_str = hex_str_from_bytes::<N, PREFIXED>(self.0);
            serializer.serialize_str(&hex_str)
        } else {
            let mut seq = serializer.serialize_tuple(N)?;
            for element in &self.0[..] {
                seq.serialize_element(element)?;
            }
            seq.end()
        }
    }
}

/// The error type returned by the inner deserialization.
// If you need `eq`, add `impl Eq fro InnerDeserializationError {}` and read warning below.
//
// For some reason `hex` (now unmaintained for > 3 years) didn't implement `Eq`, even though
// there's no reason not too, so we can't use `derive(Eq)` unfortunately.
// Note that adding the impl is risky, because if at some point `hex` decide to add non-Eq
// things to the error, then combined with the manual `impl Eq` this will create very nasty bugs.
// So, for prudence, we'll hold off on adding `Eq` until we have a good reason to.
// Existing (ignored) issue on this: https://github.com/KokaKiwi/rust-hex/issues/76.
#[derive(thiserror::Error, Clone, Debug, PartialEq)]
pub enum InnerDeserializationError {
    /// Error parsing the hex string.
    #[error(transparent)]
    FromHex(#[from] hex::FromHexError),
    /// Missing 0x prefix in the hex string.
    #[error("Missing prefix 0x in {hex_str}")]
    MissingPrefix { hex_str: String },
    /// Unexpected input byte count.
    #[error("Bad input - expected #bytes: {expected_byte_count}, string found: {string_found}.")]
    BadInput { expected_byte_count: usize, string_found: String },
}

/// Deserializes a Hex decoded as string to a byte array.
pub fn bytes_from_hex_str<const N: usize, const PREFIXED: bool>(
    hex_str: &str,
) -> Result<[u8; N], InnerDeserializationError> {
    let hex_str = if PREFIXED {
        hex_str
            .strip_prefix("0x")
            .ok_or(InnerDeserializationError::MissingPrefix { hex_str: hex_str.into() })?
    } else {
        hex_str
    };

    // Make sure string is not too long.
    if hex_str.len() > 2 * N {
        let mut err_str = "0x".to_owned();
        err_str.push_str(hex_str);
        return Err(InnerDeserializationError::BadInput {
            expected_byte_count: N,
            string_found: err_str,
        });
    }

    // Pad if needed.
    let to_add = 2 * N - hex_str.len();
    let padded_str = vec!["0"; to_add].join("") + hex_str;

    Ok(hex::decode(padded_str)?.try_into().expect("Unexpected length of deserialized hex bytes."))
}

/// Encodes a byte array to a string.
pub fn hex_str_from_bytes<const N: usize, const PREFIXED: bool>(bytes: [u8; N]) -> String {
    let hex_str = hex::encode(bytes);
    let mut hex_str = hex_str.trim_start_matches('0');
    hex_str = if hex_str.is_empty() { "0" } else { hex_str };
    if PREFIXED { format!("0x{hex_str}") } else { hex_str.to_string() }
}

pub fn deserialize_optional_contract_class_abi_entry_vector<'de, D>(
    deserializer: D,
) -> Result<Option<Vec<ContractClassAbiEntry>>, D::Error>
where
    D: Deserializer<'de>,
{
    // Deserialize the field as an `Option<Vec<ContractClassAbiEntry>>`
    let result: Result<Option<Vec<ContractClassAbiEntry>>, _> = Option::deserialize(deserializer);

    // If the field contains junk or an invalid value, return `None`.
    match result {
        Ok(value) => Ok(value),
        Err(_) => Ok(None),
    }
}

/// In old transactions, the resource bounds names are lowercase.
/// Need to convert to uppercase for deserialization to work.
///
/// Input may be attacker-controlled (e.g. an unparsable transaction body received over HTTP), so
/// a missing or non-object `resource_bounds` field must not panic; the transaction is left
/// unchanged and the typed deserialization that follows reports the error.
fn upper_case_resource_bounds_names(raw_transaction: &mut Value) {
    let Some(resource_bounds) =
        raw_transaction.get_mut("resource_bounds").and_then(Value::as_object_mut)
    else {
        return;
    };

    for (lower_case_name, upper_case_name) in
        [("l1_gas", "L1_GAS"), ("l2_gas", "L2_GAS"), ("l1_data_gas", "L1_DATA_GAS")]
    {
        if let Some(resource_bounds_value) = resource_bounds.remove(lower_case_name) {
            resource_bounds.insert(upper_case_name.to_string(), resource_bounds_value);
        }
    }
}

/// Deserializes a transaction JSON object, as returned by RPC and feeder-gateway endpoints, into
/// a [`Transaction`]. Handles legacy formats: lowercase resource bounds names in old V3
/// transactions, and a redundant zeroed `l1_data_gas` added by RPC v8.
pub fn deserialize_transaction_json_to_starknet_api_tx(
    mut raw_transaction: Value,
) -> serde_json::Result<Transaction> {
    let tx_type: String = serde_json::from_value(raw_transaction["type"].clone())?;
    let tx_version: String = serde_json::from_value(raw_transaction["version"].clone())?;

    // rpc_v8 fix (remove redundantly added L1DataGas)
    let raw_resourcebounds = &raw_transaction["resource_bounds"];
    if !raw_resourcebounds.is_null()
        && !raw_resourcebounds["l1_data_gas"].is_null()
        && raw_resourcebounds["l1_data_gas"]["max_amount"] == "0x0"
        && !raw_resourcebounds["l2_gas"].is_null()
        && raw_resourcebounds["l2_gas"]["max_amount"] == "0x0"
    {
        raw_transaction["resource_bounds"]
            .as_object_mut()
            .expect("should be map of resource bounds")
            .remove("l1_data_gas");
    }

    match (tx_type.as_str(), tx_version.as_str()) {
        ("INVOKE", "0x0") => {
            Ok(Transaction::Invoke(InvokeTransaction::V0(serde_json::from_value(raw_transaction)?)))
        }
        ("INVOKE", "0x1") => {
            Ok(Transaction::Invoke(InvokeTransaction::V1(serde_json::from_value(raw_transaction)?)))
        }
        ("INVOKE", "0x3") => {
            // In old invoke v3 transaction, the resource bounds names are lowercase.
            upper_case_resource_bounds_names(&mut raw_transaction);
            Ok(Transaction::Invoke(InvokeTransaction::V3(serde_json::from_value(raw_transaction)?)))
        }
        ("DEPLOY_ACCOUNT", "0x1") => Ok(Transaction::DeployAccount(DeployAccountTransaction::V1(
            serde_json::from_value(raw_transaction)?,
        ))),
        ("DEPLOY_ACCOUNT", "0x3") => {
            // In old deploy account v3 transaction, the resource bounds names are lowercase.
            upper_case_resource_bounds_names(&mut raw_transaction);
            Ok(Transaction::DeployAccount(DeployAccountTransaction::V3(serde_json::from_value(
                raw_transaction,
            )?)))
        }
        ("DECLARE", "0x0") => Ok(Transaction::Declare(DeclareTransaction::V0(
            serde_json::from_value(raw_transaction)?,
        ))),
        ("DECLARE", "0x1") => Ok(Transaction::Declare(DeclareTransaction::V1(
            serde_json::from_value(raw_transaction)?,
        ))),
        ("DECLARE", "0x2") => Ok(Transaction::Declare(DeclareTransaction::V2(
            serde_json::from_value(raw_transaction)?,
        ))),
        ("DECLARE", "0x3") => {
            // In old declare v3 transaction, the resource bounds names are lowercase.
            upper_case_resource_bounds_names(&mut raw_transaction);
            Ok(Transaction::Declare(DeclareTransaction::V3(serde_json::from_value(
                raw_transaction,
            )?)))
        }
        ("L1_HANDLER", _) => Ok(Transaction::L1Handler(serde_json::from_value(raw_transaction)?)),
        (tx_type, tx_version) => Err(serde::de::Error::custom(format!(
            "unimplemented transaction type: {tx_type} version: {tx_version}"
        ))),
    }
}
