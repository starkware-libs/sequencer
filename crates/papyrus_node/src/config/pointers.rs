use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::mem::discriminant;
use std::ops::IndexMut;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::Duration;
use std::{env, fs, io};

use apollo_central_sync::sources::central::CentralSourceConfig;
use apollo_central_sync::SyncConfig;
#[cfg(not(feature = "rpc"))]
use apollo_config::dumping::ser_param;
use apollo_config::dumping::{
    prepend_sub_config_name,
    ser_optional_sub_config,
    ser_pointer_target_param,
    ser_pointer_target_required_param,
    set_pointing_param_paths,
    ConfigPointers,
    Pointers,
    SerializeConfig,
};
use apollo_config::loading::load_and_process_config;
#[cfg(not(feature = "rpc"))]
use apollo_config::ParamPrivacyInput;
use apollo_config::{ConfigError, ParamPath, SerializationType, SerializedParam};
use apollo_network::NetworkConfig;
use apollo_p2p_sync::client::{P2pSyncClient, P2pSyncClientConfig};
#[cfg(feature = "rpc")]
use apollo_rpc::RpcConfig;
use apollo_starknet_client::RetryConfig;
use apollo_storage::db::DbConfig;
use apollo_storage::StorageConfig;
use clap::{arg, value_parser, Arg, ArgMatches, Command};
use itertools::{chain, Itertools};
use lazy_static::lazy_static;
use papyrus_base_layer::ethereum_base_layer_contract::EthereumBaseLayerConfig;
use papyrus_monitoring_gateway::MonitoringGatewayConfig;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use starknet_api::core::ChainId;
use validator::Validate;

use crate::version::VERSION_FULL;

/// Returns vector of `(pointer target name, pointer target serialized param, vec<pointer param
/// path>)` to be applied on the dumped node config.
/// The config updates will be performed on the shared pointer targets, and finally, the values
/// will be propagated to the pointer params.
pub static CONFIG_POINTERS: LazyLock<ConfigPointers> = LazyLock::new(|| {
    vec![
        (
            ser_pointer_target_required_param(
                "chain_id",
                SerializationType::String,
                "The chain to follow. For more details see https://docs.starknet.io/documentation/architecture_and_concepts/Blocks/transactions/#chain-id.",
            ),
            set_pointing_param_paths(&[
                "context.chain_id",
                "network.chain_id",
                "rpc.chain_id",
                "storage.db_config.chain_id",
            ]),
        ),
        (
            ser_pointer_target_param(
                "starknet_url",
                &"https://feeder.alpha-mainnet.starknet.io/".to_string(),
                "The URL of a centralized Starknet gateway.",
            ),
            set_pointing_param_paths(&[
                "rpc.starknet_url",
                "central.starknet_url",
                "monitoring_gateway.starknet_url",
            ]),
        ),
        (
            ser_pointer_target_param(
                "collect_metrics",
                &false,
                "If true, collect metrics for the node.",
            ),
            set_pointing_param_paths(&[
                "rpc.collect_metrics",
                "monitoring_gateway.collect_metrics",
            ]),
        ),
    ]
});

/// Parameters that should 1) not be pointers, and 2) have a name matching a pointer target
/// param. Used in verification.
pub static CONFIG_NON_POINTERS_WHITELIST: LazyLock<Pointers> =
    LazyLock::new(HashSet::<ParamPath>::new);
