use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use size_of::SizeOf;
use starknet_types_core::felt::Felt;
use strum_macros::EnumIter;

use crate::block::{GasPrice, NonzeroGasPrice};
use crate::execution_resources::{GasAmount, GasVector};
use crate::hash::StarkHash;
use crate::serde_utils::PrefixedBytesAsHex;
use crate::{StarknetApiError, StarknetApiResult};

pub const HIGH_GAS_AMOUNT: u64 = 10000000000; // A high gas amount that should be enough for execution.

/// A fee.
#[cfg_attr(any(test, feature = "testing"), derive(derive_more::Add, derive_more::Deref))]
#[derive(
    Debug,
    Copy,
    Clone,
    Default,
    derive_more::Display,
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

    pub const fn saturating_add(self, rhs: Self) -> Self {
        Self(self.0.saturating_add(rhs.0))
    }

    pub fn checked_div_ceil(self, rhs: NonzeroGasPrice) -> Option<GasAmount> {
        self.checked_div(rhs).map(|value| {
            if value
                .checked_mul(rhs.into())
                .expect("Multiplying by denominator of floor division cannot overflow.")
                < self
            {
                (value.0 + 1).into()
            } else {
                value
            }
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

/// A contract address salt.
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
    SizeOf,
)]
pub struct ContractAddressSalt(pub StarkHash);

/// A transaction signature, wrapped in `Arc` for efficient cloning and safe sharing across threads.
/// `Rc` is avoided due to its lack of thread safety, and `Mutex` is unnecessary as the signature
/// vector is immutable and never modified.
#[derive(
    Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord, SizeOf,
)]
pub struct TransactionSignature(pub Arc<Vec<Felt>>);

/// The calldata of a transaction.
#[derive(
    Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord, SizeOf,
)]
pub struct Calldata(pub Arc<Vec<Felt>>);

#[macro_export]
macro_rules! calldata {
    ( $( $x:expr ),* ) => {
        $crate::transaction::fields::Calldata(vec![$($x),*].into())
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
    derive_more::Display,
    SizeOf,
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

impl From<Tip> for GasPrice {
    fn from(tip: Tip) -> Self {
        Self(tip.0.into())
    }
}

impl Tip {
    pub const ZERO: Self = Self(0);
}

/// Execution resource.
#[derive(
    Clone,
    Copy,
    Debug,
    Deserialize,
    derive_more::Display,
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
    #[serde(alias = "L1_DATA_GAS")]
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
    SizeOf,
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

impl std::fmt::Display for ResourceBounds {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{{ max_amount: {}, max_price_per_unit: {} }}",
            self.max_amount, self.max_price_per_unit
        )
    }
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

pub fn hex_to_tip<'de, D>(deserializer: D) -> Result<Tip, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    Ok(Tip(u64::from_str_radix(s.trim_start_matches("0x"), 16).map_err(serde::de::Error::custom)?))
}

pub struct ResourceAsFelts {
    pub resource_name: Felt,
    pub max_amount: Felt,
    pub max_price_per_unit: Felt,
}

impl ResourceAsFelts {
    pub fn new(resource: Resource, resource_bounds: &ResourceBounds) -> StarknetApiResult<Self> {
        let resource_as_hex = resource.to_hex();
        Ok(Self {
            resource_name: Felt::from_hex(resource_as_hex).map_err(|_| {
                StarknetApiError::ResourceHexToFeltConversion(resource_as_hex.to_string())
            })?,
            max_amount: Felt::from(resource_bounds.max_amount),
            max_price_per_unit: Felt::from(resource_bounds.max_price_per_unit),
        })
    }

    pub fn flatten(self) -> Vec<Felt> {
        vec![self.resource_name, self.max_amount, self.max_price_per_unit]
    }
}

pub fn valid_resource_bounds_as_felts(
    resource_bounds: &ValidResourceBounds,
    exclude_l1_data_gas: bool,
) -> Result<Vec<ResourceAsFelts>, StarknetApiError> {
    let mut resource_bounds_vec: Vec<_> = vec![
        ResourceAsFelts::new(Resource::L1Gas, &resource_bounds.get_l1_bounds())?,
        ResourceAsFelts::new(Resource::L2Gas, &resource_bounds.get_l2_bounds())?,
    ];
    if exclude_l1_data_gas {
        return Ok(resource_bounds_vec);
    }
    if let ValidResourceBounds::AllResources(AllResourceBounds { l1_data_gas, .. }) =
        resource_bounds
    {
        resource_bounds_vec.push(ResourceAsFelts::new(Resource::L1DataGas, l1_data_gas)?)
    }
    Ok(resource_bounds_vec)
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
    pub fn max_possible_fee(&self, tip: Tip) -> Fee {
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
                .saturating_add(
                    l2_gas
                        .max_amount
                        .saturating_mul(l2_gas.max_price_per_unit.saturating_add(tip.into())),
                )
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

    pub fn new_unlimited_gas_no_fee_enforcement() -> Self {
        let default_l2_gas_amount = GasAmount(HIGH_GAS_AMOUNT); // Sufficient to avoid out of gas errors.
        let default_resource =
            ResourceBounds { max_amount: GasAmount(0), max_price_per_unit: GasPrice(1) };
        Self::AllResources(AllResourceBounds {
            l2_gas: ResourceBounds {
                max_amount: default_l2_gas_amount,
                max_price_per_unit: GasPrice(0), // Set to zero for no enforce_fee mechanism.
            },
            l1_gas: default_resource,
            l1_data_gas: default_resource,
        })
    }

    #[cfg(any(feature = "testing", test))]
    pub fn create_for_testing_no_fee_enforcement() -> Self {
        Self::new_unlimited_gas_no_fee_enforcement()
    }

    /// Utility method to "zip" an amount vector and a price vector to get an AllResourceBounds.
    #[cfg(any(feature = "testing", test))]
    pub fn all_bounds_from_vectors(
        gas: &crate::execution_resources::GasVector,
        prices: &crate::block::GasPriceVector,
    ) -> Self {
        let l1_gas = ResourceBounds {
            max_amount: gas.l1_gas,
            max_price_per_unit: prices.l1_gas_price.into(),
        };
        let l2_gas = ResourceBounds {
            max_amount: gas.l2_gas,
            max_price_per_unit: prices.l2_gas_price.into(),
        };
        let l1_data_gas = ResourceBounds {
            max_amount: gas.l1_data_gas,
            max_price_per_unit: prices.l1_data_gas_price.into(),
        };
        Self::AllResources(AllResourceBounds { l1_gas, l2_gas, l1_data_gas })
    }
}

impl Default for ValidResourceBounds {
    fn default() -> Self {
        Self::AllResources(AllResourceBounds::default())
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    Deserialize,
    Eq,
    PartialEq,
    Hash,
    Ord,
    PartialOrd,
    Serialize,
    SizeOf,
)]
pub struct AllResourceBounds {
    pub l1_gas: ResourceBounds,
    pub l2_gas: ResourceBounds,
    pub l1_data_gas: ResourceBounds,
}

impl AllResourceBounds {
    #[cfg(any(feature = "testing", test))]
    pub fn create_for_testing() -> Self {
        let resource_bounds =
            ResourceBounds { max_amount: GasAmount(0), max_price_per_unit: GasPrice(1) };
        Self { l1_gas: resource_bounds, l2_gas: resource_bounds, l1_data_gas: resource_bounds }
    }
}

impl std::fmt::Display for AllResourceBounds {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{{ l1_gas: {}, l2_gas: {}, l1_data_gas: {} }}",
            self.l1_gas, self.l2_gas, self.l1_data_gas
        )
    }
}

impl AllResourceBounds {
    pub fn get_bound(&self, resource: Resource) -> ResourceBounds {
        match resource {
            Resource::L1Gas => self.l1_gas,
            Resource::L2Gas => self.l2_gas,
            Resource::L1DataGas => self.l1_data_gas,
        }
    }

    pub fn to_max_amounts(&self) -> GasVector {
        GasVector {
            l1_gas: self.l1_gas.max_amount,
            l1_data_gas: self.l1_data_gas.max_amount,
            l2_gas: self.l2_gas.max_amount,
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
#[derive(
    Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, SizeOf,
)]
pub struct PaymasterData(pub Vec<Felt>);

impl PaymasterData {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// If nonempty, will contain the required data for deploying and initializing an account contract:
/// its class hash, address salt and constructor calldata.
#[derive(
    Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord, SizeOf,
)]
pub struct AccountDeploymentData(pub Vec<Felt>);

impl AccountDeploymentData {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
