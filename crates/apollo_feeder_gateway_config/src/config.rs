use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr};

#[cfg(test)]
#[path = "config_test.rs"]
mod config_test;

use apollo_config::dumping::{
    prepend_sub_config_name,
    ser_optional_param,
    ser_param,
    SerializeConfig,
};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use starknet_api::core::{ContractAddress, EthAddress, SequencerPublicKey};
use starknet_api::hash::StarkHash;
use validator::Validate;

const FEEDER_GATEWAY_PORT: u16 = 8082; // configurable; intentionally NOT legacy 9713.

// Schema placeholder for the `read_pool_size` default. A value of 0 (or an unset field) means the
// pool size is derived at runtime as ~1.5x the available CPUs; see `read_pool_size`.
const AUTO_READ_POOL_SIZE: usize = 0;

/// Selects how the feeder gateway reads chain data, chosen by deployment topology.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReadBackend {
    /// Read directly from co-located `apollo_storage` in the same process (highest throughput).
    #[default]
    Colocated,
    /// Read remotely from the state-sync process (different pod/node).
    Remote,
}

/// The well-known contract addresses served by `get_contract_addresses`.
///
/// The live Python feeder gateway's response is NETWORK-VARIABLE: an ordered map of well-known L1
/// contracts (mainnet serves 4 keys, sepolia 8, in different orders; verified live 2026-06-03)
/// followed by the two L2 fee-token address felts. The L1 contract set and its order are
/// therefore plain configuration.
#[derive(Clone, Debug, Default, Serialize, Deserialize, Validate, PartialEq)]
pub struct FeederGatewayContractAddresses {
    /// Ordered `(name, address)` pairs of the network's well-known L1 contracts; the endpoint
    /// serves them in this order, EIP-55 checksummed. Configured as a space-separated
    /// `Name:0xaddress` string.
    #[serde(
        serialize_with = "serialize_l1_contract_addresses",
        deserialize_with = "deserialize_l1_contract_addresses"
    )]
    pub l1_contract_addresses: Vec<(String, EthAddress)>,
    pub strk_l2_token_address: ContractAddress,
    pub eth_l2_token_address: ContractAddress,
}

/// Formats the ordered L1 contract list as its space-separated `Name:0xaddress` config form
/// (which preserves order, unlike a JSON object through `serde_json`'s sorted map).
fn l1_contract_addresses_to_string(l1_contract_addresses: &[(String, EthAddress)]) -> String {
    l1_contract_addresses
        .iter()
        .map(|(contract_name, address)| format!("{contract_name}:0x{:x}", address.0))
        .collect::<Vec<_>>()
        .join(" ")
}

fn serialize_l1_contract_addresses<S: Serializer>(
    l1_contract_addresses: &[(String, EthAddress)],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&l1_contract_addresses_to_string(l1_contract_addresses))
}

fn deserialize_l1_contract_addresses<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<(String, EthAddress)>, D::Error> {
    let raw_pairs: String = Deserialize::deserialize(deserializer)?;
    if raw_pairs.is_empty() {
        return Ok(Vec::new());
    }
    raw_pairs
        .split(' ')
        .map(|raw_pair| {
            let (contract_name, address_hex) = raw_pair.split_once(':').ok_or_else(|| {
                D::Error::custom(format!("pair \"{raw_pair}\" is not in Name:0xaddress form"))
            })?;
            let address = StarkHash::from_hex(address_hex)
                .map_err(|error| {
                    D::Error::custom(format!("invalid address in \"{raw_pair}\": {error}"))
                })
                .and_then(|felt_value| {
                    EthAddress::try_from(felt_value).map_err(|error| {
                        D::Error::custom(format!("invalid address in \"{raw_pair}\": {error}"))
                    })
                })?;
            Ok((contract_name.to_string(), address))
        })
        .collect()
}

impl SerializeConfig for FeederGatewayContractAddresses {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "l1_contract_addresses",
                &l1_contract_addresses_to_string(&self.l1_contract_addresses),
                "Ordered space-separated Name:0xaddress pairs of the network's well-known L1 \
                 contracts, served in this order (EIP-55 checksummed) by get_contract_addresses.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "strk_l2_token_address",
                &self.strk_l2_token_address,
                "The STRK fee token L2 address.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "eth_l2_token_address",
                &self.eth_l2_token_address,
                "The ETH fee token L2 address.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct FeederGatewayConfig {
    pub ip: IpAddr,
    pub port: u16,
    pub read_backend: ReadBackend,
    /// Maximum number of concurrent blocking reads. When unset (or 0), derived at runtime as ~1.5x
    /// the available CPUs.
    pub read_pool_size: Option<usize>,
    #[validate(nested)]
    pub contract_addresses: FeederGatewayContractAddresses,
    /// The sequencer public key served by `get_public_key` (a bare felt). Network-specific.
    pub sequencer_public_key: SequencerPublicKey,
}

impl Default for FeederGatewayConfig {
    fn default() -> Self {
        Self {
            ip: IpAddr::from(Ipv4Addr::UNSPECIFIED),
            port: FEEDER_GATEWAY_PORT,
            read_backend: ReadBackend::default(),
            read_pool_size: None,
            contract_addresses: FeederGatewayContractAddresses::default(),
            sequencer_public_key: SequencerPublicKey::default(),
        }
    }
}

impl SerializeConfig for FeederGatewayConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut dump = BTreeMap::from_iter([
            ser_param(
                "ip",
                &self.ip.to_string(),
                "The feeder gateway ip.",
                ParamPrivacyInput::Public,
            ),
            ser_param("port", &self.port, "The feeder gateway port.", ParamPrivacyInput::Public),
            ser_param(
                "read_backend",
                &self.read_backend,
                "The feeder gateway read backend (Colocated reads local storage; Remote reads via \
                 the state sync client).",
                ParamPrivacyInput::Public,
            ),
        ]);
        dump.extend(ser_optional_param(
            &self.read_pool_size,
            AUTO_READ_POOL_SIZE,
            "read_pool_size",
            "Maximum number of concurrent blocking reads. 0 (or unset) derives ~1.5x available \
             CPUs at runtime.",
            ParamPrivacyInput::Public,
        ));
        dump.extend(prepend_sub_config_name(self.contract_addresses.dump(), "contract_addresses"));
        dump.extend([ser_param(
            "sequencer_public_key",
            &self.sequencer_public_key,
            "The sequencer public key served by get_public_key.",
            ParamPrivacyInput::Public,
        )]);
        dump
    }
}

impl FeederGatewayConfig {
    pub fn ip_and_port(&self) -> (IpAddr, u16) {
        (self.ip, self.port)
    }

    /// The effective read pool size: the configured value, or ~1.5x the available CPUs when unset
    /// or 0.
    pub fn read_pool_size(&self) -> usize {
        match self.read_pool_size {
            Some(size) if size > 0 => size,
            _ => default_read_pool_size(),
        }
    }
}

fn default_read_pool_size() -> usize {
    let cores = std::thread::available_parallelism().map(|cores| cores.get()).unwrap_or(1);
    // ~1.5x cores: MDBX reads are CPU/page-cache-bound, so an oversized pool just context-switches.
    (cores * 3) / 2
}
