use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr};

use apollo_config::dumping::{
    prepend_sub_config_name,
    ser_optional_param,
    ser_param,
    SerializeConfig,
};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_api::core::ContractAddress;
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

/// The well-known contract addresses served by `get_contract_addresses`. The field names use the
/// Python feeder gateway's JSON key casing, since this struct is serialized directly into that
/// endpoint's response.
#[derive(Clone, Debug, Default, Serialize, Deserialize, Validate, PartialEq)]
pub struct FeederGatewayContractAddresses {
    #[serde(rename = "Starknet")]
    pub starknet: ContractAddress,
    #[serde(rename = "GpsStatementVerifier")]
    pub gps_statement_verifier: ContractAddress,
}

impl SerializeConfig for FeederGatewayContractAddresses {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "starknet",
                &self.starknet,
                "The Starknet core contract address.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "gps_statement_verifier",
                &self.gps_statement_verifier,
                "The GPS statement verifier contract address.",
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
}

impl Default for FeederGatewayConfig {
    fn default() -> Self {
        Self {
            ip: IpAddr::from(Ipv4Addr::UNSPECIFIED),
            port: FEEDER_GATEWAY_PORT,
            read_backend: ReadBackend::default(),
            read_pool_size: None,
            contract_addresses: FeederGatewayContractAddresses::default(),
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
