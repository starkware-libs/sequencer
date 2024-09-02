use std::collections::BTreeMap;
use std::fmt::Display;
use std::sync::Arc;

use derive_more::Display;
use num_bigint::BigUint;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use starknet_types_core::felt::Felt;
use strum_macros::EnumIter;

use super::Transaction;
use crate::block::{GasPrice, NonzeroGasPrice};
use crate::execution_resources::GasAmount;
use crate::hash::StarkHash;
use crate::serde_utils::PrefixedBytesAsHex;
use crate::StarknetApiError;

// TODO(Noa, 14/11/2023): Replace QUERY_VERSION_BASE_BIT with a lazy calculation.
//      pub static QUERY_VERSION_BASE: Lazy<Felt> = ...
pub const QUERY_VERSION_BASE_BIT: u32 = 128;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
pub struct TransactionOptions {
    /// Transaction that shouldn't be broadcasted to StarkNet. For example, users that want to
    /// test the execution result of a transaction without the risk of it being rebroadcasted (the
    /// signature will be different while the execution remain the same). Using this flag will
    /// modify the transaction version by setting the 128-th bit to 1.
    pub only_query: bool,
}

/// A fee.
#[cfg_attr(any(test, feature = "testing"), derive(derive_more::Add, derive_more::Deref))]
#[derive(
    Debug,
    Copy,
    Clone,
    Default,
    Display,
    Eq,
    PartialEq,
    Hash,
    Deserialize,
    Serialize,
    PartialOrd,
    Ord,
)]
#[serde(from = "PrefixedBytesAsHex<16_usize>", into = "PrefixedBytesAsHex<16_usize>")]
pub struct Fee(pub u128);

impl Fee {
    pub fn checked_add(self, rhs: Fee) -> Option<Fee> {
        self.0.checked_add(rhs.0).map(Fee)
    }

    pub fn saturating_add(self, rhs: Self) -> Self {
        Self(self.0.saturating_add(rhs.0))
    }

    pub fn checked_div_ceil(self, rhs: NonzeroGasPrice) -> Option<GasAmount> {
        self.checked_div(rhs).map(|value| {
            if value.nonzero_saturating_mul(rhs) < self { (value.0 + 1).into() } else { value }
        })
    }

    pub fn checked_div(self, rhs: NonzeroGasPrice) -> Option<GasAmount> {
        match u64::try_from(self.0 / rhs.get().0) {
            Ok(value) => Some(value.into()),
            Err(_) => None,
        }
    }

    pub fn saturating_div(self, rhs: NonzeroGasPrice) -> GasAmount {
        self.checked_div(rhs).unwrap_or(GasAmount::MAX)
    }
}

impl From<PrefixedBytesAsHex<16_usize>> for Fee {
    fn from(value: PrefixedBytesAsHex<16_usize>) -> Self {
        Self(u128::from_be_bytes(value.0))
    }
}

impl From<Fee> for PrefixedBytesAsHex<16_usize> {
    fn from(fee: Fee) -> Self {
        Self(fee.0.to_be_bytes())
    }
}

impl From<Fee> for Felt {
    fn from(fee: Fee) -> Self {
        Self::from(fee.0)
    }
}

/// The hash of a [Transaction](`crate::transaction::Transaction`).
#[derive(
    Debug,
    Default,
    Copy,
    Clone,
    Eq,
    PartialEq,
    Hash,
    Deserialize,
    Serialize,
    PartialOrd,
    Ord,
    derive_more::Deref,
)]
pub struct TransactionHash(pub StarkHash);

impl Display for TransactionHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A contract address salt.
#[derive(
    Debug, Copy, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct ContractAddressSalt(pub StarkHash);

/// A transaction signature.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct TransactionSignature(pub Vec<Felt>);

/// A transaction version.
#[derive(
    Debug,
    Copy,
    Clone,
    Default,
    Eq,
    PartialEq,
    Hash,
    Deserialize,
    Serialize,
    PartialOrd,
    Ord,
    derive_more::Deref,
)]
pub struct TransactionVersion(pub Felt);

impl TransactionVersion {
    /// [TransactionVersion] constant that's equal to 0.
    pub const ZERO: Self = { Self(Felt::ZERO) };

    /// [TransactionVersion] constant that's equal to 1.
    pub const ONE: Self = { Self(Felt::ONE) };

    /// [TransactionVersion] constant that's equal to 2.
    pub const TWO: Self = { Self(Felt::TWO) };

    /// [TransactionVersion] constant that's equal to 3.
    pub const THREE: Self = { Self(Felt::THREE) };
}

// TODO: TransactionVersion and SignedTransactionVersion should probably be separate types.
// Returns the transaction version taking into account the transaction options.
pub fn signed_tx_version_from_tx(
    tx: &Transaction,
    transaction_options: &TransactionOptions,
) -> TransactionVersion {
    signed_tx_version(&tx.version(), transaction_options)
}

pub fn signed_tx_version(
    tx_version: &TransactionVersion,
    transaction_options: &TransactionOptions,
) -> TransactionVersion {
    // If only_query is true, set the 128-th bit.
    let query_only_bit = Felt::TWO.pow(QUERY_VERSION_BASE_BIT);
    assert_eq!(
        tx_version.0.to_biguint() & query_only_bit.to_biguint(),
        BigUint::from(0_u8),
        "Requested signed tx version with version that already has query bit set: {tx_version:?}."
    );
    if transaction_options.only_query {
        TransactionVersion(tx_version.0 + query_only_bit)
    } else {
        *tx_version
    }
}

/// The calldata of a transaction.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct Calldata(pub Arc<Vec<Felt>>);

#[macro_export]
macro_rules! calldata {
    ( $( $x:expr ),* ) => {
        Calldata(vec![$($x),*].into())
    };
}

/// Transaction fee tip.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    Deserialize,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    Serialize,
    derive_more::Deref,
)]
#[serde(from = "PrefixedBytesAsHex<8_usize>", into = "PrefixedBytesAsHex<8_usize>")]
pub struct Tip(pub u64);

impl From<PrefixedBytesAsHex<8_usize>> for Tip {
    fn from(value: PrefixedBytesAsHex<8_usize>) -> Self {
        Self(u64::from_be_bytes(value.0))
    }
}

impl From<Tip> for PrefixedBytesAsHex<8_usize> {
    fn from(tip: Tip) -> Self {
        Self(tip.0.to_be_bytes())
    }
}

impl From<Tip> for Felt {
    fn from(tip: Tip) -> Self {
        Self::from(tip.0)
    }
}

/// Execution resource.
#[derive(
    Clone,
    Copy,
    Debug,
    Deserialize,
    Display,
    EnumIter,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    Serialize,
)]
pub enum Resource {
    #[serde(rename = "L1_GAS")]
    L1Gas,
    #[serde(rename = "L2_GAS")]
    L2Gas,
    #[serde(rename = "L1_DATA")]
    L1DataGas,
}

impl Resource {
    pub fn to_hex(&self) -> &'static str {
        match self {
            Resource::L1Gas => "0x00000000000000000000000000000000000000000000000000004c315f474153",
            Resource::L2Gas => "0x00000000000000000000000000000000000000000000000000004c325f474153",
            Resource::L1DataGas => {
                "0x000000000000000000000000000000000000000000000000004c315f44415441"
            }
        }
    }
}

/// Fee bounds for an execution resource.
/// TODO(Yael): add types ResourceAmount and ResourcePrice and use them instead of u64 and u128.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
// TODO(Nimrod): Consider renaming this struct.
pub struct ResourceBounds {
    // Specifies the maximum amount of each resource allowed for usage during the execution.
    #[serde(serialize_with = "gas_amount_to_hex", deserialize_with = "hex_to_gas_amount")]
    pub max_amount: GasAmount,

    // Specifies the maximum price the user is willing to pay for each resource unit.
    #[serde(serialize_with = "gas_price_to_hex", deserialize_with = "hex_to_gas_price")]
    pub max_price_per_unit: GasPrice,
}

impl ResourceBounds {
    /// Returns true iff both the max amount and the max amount per unit is zero.
    pub fn is_zero(&self) -> bool {
        self.max_amount == GasAmount(0) && self.max_price_per_unit == GasPrice(0)
    }
}

fn gas_amount_to_hex<S>(value: &GasAmount, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&format!("0x{:x}", value.0))
}

fn hex_to_gas_amount<'de, D>(deserializer: D) -> Result<GasAmount, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    Ok(GasAmount(
        u64::from_str_radix(s.trim_start_matches("0x"), 16).map_err(serde::de::Error::custom)?,
    ))
}

fn gas_price_to_hex<S>(value: &GasPrice, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&format!("0x{:x}", value.0))
}

fn hex_to_gas_price<'de, D>(deserializer: D) -> Result<GasPrice, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    Ok(GasPrice(
        u128::from_str_radix(s.trim_start_matches("0x"), 16).map_err(serde::de::Error::custom)?,
    ))
}

#[derive(Debug, PartialEq)]
pub enum GasVectorComputationMode {
    All,
    NoL2Gas,
}

/// A mapping from execution resources to their corresponding fee bounds..
#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
// TODO(Nimrod): Remove this struct definition.
pub struct DeprecatedResourceBoundsMapping(pub BTreeMap<Resource, ResourceBounds>);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum ValidResourceBounds {
    L1Gas(ResourceBounds), // Pre 0.13.3. Only L1 gas. L2 bounds are signed but never used.
    AllResources(AllResourceBounds),
}

impl ValidResourceBounds {
    pub fn get_l1_bounds(&self) -> ResourceBounds {
        match self {
            Self::L1Gas(l1_bounds) => *l1_bounds,
            Self::AllResources(AllResourceBounds { l1_gas, .. }) => *l1_gas,
        }
    }

    pub fn get_l2_bounds(&self) -> ResourceBounds {
        match self {
            Self::L1Gas(_) => ResourceBounds::default(),
            Self::AllResources(AllResourceBounds { l2_gas, .. }) => *l2_gas,
        }
    }

    /// Returns the maximum possible fee that can be charged for the transaction.
    /// The computation is saturating, meaning that if the result is larger than the maximum
    /// possible fee, the maximum possible fee is returned.
    pub fn max_possible_fee(&self) -> Fee {
        match self {
            ValidResourceBounds::L1Gas(l1_bounds) => {
                l1_bounds.max_amount.saturating_mul(l1_bounds.max_price_per_unit)
            }
            ValidResourceBounds::AllResources(AllResourceBounds {
                l1_gas,
                l2_gas,
                l1_data_gas,
            }) => l1_gas
                .max_amount
                .saturating_mul(l1_gas.max_price_per_unit)
                .saturating_add(l2_gas.max_amount.saturating_mul(l2_gas.max_price_per_unit))
                .saturating_add(
                    l1_data_gas.max_amount.saturating_mul(l1_data_gas.max_price_per_unit),
                ),
        }
    }

    pub fn get_gas_vector_computation_mode(&self) -> GasVectorComputationMode {
        match self {
            Self::AllResources(_) => GasVectorComputationMode::All,
            Self::L1Gas(_) => GasVectorComputationMode::NoL2Gas,
        }
    }

    // TODO(Nimrod): Default testing bounds should probably be AllResourceBounds variant.
    #[cfg(any(feature = "testing", test))]
    pub fn create_for_testing() -> Self {
        Self::L1Gas(ResourceBounds { max_amount: GasAmount(0), max_price_per_unit: GasPrice(1) })
    }
}

#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Hash, Ord, PartialOrd, Serialize,
)]
pub struct AllResourceBounds {
    pub l1_gas: ResourceBounds,
    pub l2_gas: ResourceBounds,
    pub l1_data_gas: ResourceBounds,
}

impl AllResourceBounds {
    pub fn get_bound(&self, resource: Resource) -> ResourceBounds {
        match resource {
            Resource::L1Gas => self.l1_gas,
            Resource::L2Gas => self.l2_gas,
            Resource::L1DataGas => self.l1_data_gas,
        }
    }
}

/// Deserializes raw resource bounds, given as map, into valid resource bounds.
// TODO(Nimrod): Figure out how to get same result with adding #[derive(Deserialize)].
impl<'de> Deserialize<'de> for ValidResourceBounds {
    fn deserialize<D>(de: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw_resource_bounds: BTreeMap<Resource, ResourceBounds> = Deserialize::deserialize(de)?;
        ValidResourceBounds::try_from(DeprecatedResourceBoundsMapping(raw_resource_bounds))
            .map_err(serde::de::Error::custom)
    }
}

/// Serializes ValidResourceBounds as map for Backwards compatibility.
// TODO(Nimrod): Figure out how to get same result with adding #[derive(Serialize)].
impl Serialize for ValidResourceBounds {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let map = match self {
            ValidResourceBounds::L1Gas(l1_gas) => BTreeMap::from([
                (Resource::L1Gas, *l1_gas),
                (Resource::L2Gas, ResourceBounds::default()),
            ]),
            ValidResourceBounds::AllResources(AllResourceBounds {
                l1_gas,
                l2_gas,
                l1_data_gas,
            }) => BTreeMap::from([
                (Resource::L1Gas, *l1_gas),
                (Resource::L2Gas, *l2_gas),
                (Resource::L1DataGas, *l1_data_gas),
            ]),
        };
        DeprecatedResourceBoundsMapping(map).serialize(s)
    }
}

impl TryFrom<DeprecatedResourceBoundsMapping> for ValidResourceBounds {
    type Error = StarknetApiError;
    fn try_from(
        resource_bounds_mapping: DeprecatedResourceBoundsMapping,
    ) -> Result<Self, Self::Error> {
        if let (Some(l1_bounds), Some(l2_bounds)) = (
            resource_bounds_mapping.0.get(&Resource::L1Gas),
            resource_bounds_mapping.0.get(&Resource::L2Gas),
        ) {
            match resource_bounds_mapping.0.get(&Resource::L1DataGas) {
                Some(data_bounds) => Ok(Self::AllResources(AllResourceBounds {
                    l1_gas: *l1_bounds,
                    l1_data_gas: *data_bounds,
                    l2_gas: *l2_bounds,
                })),
                None => {
                    if l2_bounds.is_zero() {
                        Ok(Self::L1Gas(*l1_bounds))
                    } else {
                        Err(StarknetApiError::InvalidResourceMappingInitializer(format!(
                            "Missing data gas bounds but L2 gas bound is not zero: \
                             {resource_bounds_mapping:?}",
                        )))
                    }
                }
            }
        } else {
            Err(StarknetApiError::InvalidResourceMappingInitializer(format!(
                "{resource_bounds_mapping:?}",
            )))
        }
    }
}

/// Paymaster-related data.
#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct PaymasterData(pub Vec<Felt>);

/// If nonempty, will contain the required data for deploying and initializing an account contract:
/// its class hash, address salt and constructor calldata.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct AccountDeploymentData(pub Vec<Felt>);
