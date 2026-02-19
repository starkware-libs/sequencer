use std::collections::HashMap;

use apollo_network_benchmark::node_args::UserArgs;
use clap::Parser;
use serde::Serialize;

/// Configuration for the orchestrator that spawns multiple nodes
#[derive(Parser, Debug, Clone, Serialize)]
pub struct SharedArgs {
    /// Number of nodes to run
    #[arg(long, default_value = "3")]
    pub num_nodes: u32,

    /// Sets the multi-addresses to use UDP/QUIC instead of TCP
    #[arg(long, default_value = "false")]
    pub quic: bool,

    #[command(flatten)]
    pub user: UserArgs,
}

/// Single source of truth: produces env var (NAME, VALUE) pairs matching the
/// clap `env` attributes on `RunnerArgs` / `UserArgs` in the node binary.
pub fn get_env_var_pairs(
    id: Option<u32>,
    metric_port: u16,
    p2p_port: u16,
    bootstrap_nodes: &[String],
    args: &SharedArgs,
    latency: Option<u32>,
    throughput: Option<u32>,
) -> Vec<(String, String)> {
    let broadcaster =
        args.user.broadcaster.and_then(|b| u32::try_from(b).ok()).unwrap_or(args.num_nodes - 1);

    let mut pairs = vec![];

    if let Some(id) = id {
        pairs.push(("ID".to_string(), id.to_string()));
    }

    pairs.extend_from_slice(&[
        ("METRIC_PORT".to_string(), metric_port.to_string()),
        ("P2P_PORT".to_string(), p2p_port.to_string()),
        ("BOOTSTRAP".to_string(), bootstrap_nodes.join(",")),
        ("TIMEOUT".to_string(), args.user.timeout.to_string()),
        ("VERBOSITY".to_string(), args.user.verbosity.to_string()),
        ("BUFFER_SIZE".to_string(), args.user.buffer_size.to_string()),
        ("MESSAGE_SIZE_BYTES".to_string(), args.user.message_size_bytes.to_string()),
        ("HEARTBEAT_MILLIS".to_string(), args.user.heartbeat_millis.to_string()),
        ("MODE".to_string(), args.user.mode.to_string()),
        ("NETWORK_PROTOCOL".to_string(), args.user.network_protocol.to_string()),
        ("BROADCASTER".to_string(), broadcaster.to_string()),
        ("ROUND_DURATION_SECONDS".to_string(), args.user.round_duration_seconds.to_string()),
        ("NUM_NODES".to_string(), args.num_nodes.to_string()),
    ]);

    if let Some(latency) = latency {
        pairs.push(("LATENCY".to_string(), latency.to_string()));
    }
    if let Some(throughput) = throughput {
        pairs.push(("THROUGHPUT".to_string(), throughput.to_string()));
    }

    pairs
}

/// Wraps `get_env_var_pairs` into the K8s container env format:
/// `[{"name": "FOO", "value": "bar"}, ...]`
pub fn get_k8s_env_vars(
    id: Option<u32>,
    metric_port: u16,
    p2p_port: u16,
    bootstrap_nodes: &[String],
    args: &SharedArgs,
    latency: Option<u32>,
    throughput: Option<u32>,
) -> Vec<HashMap<String, String>> {
    get_env_var_pairs(id, metric_port, p2p_port, bootstrap_nodes, args, latency, throughput)
        .into_iter()
        .map(|(name, value)| {
            HashMap::from([("name".to_string(), name), ("value".to_string(), value)])
        })
        .collect()
}
