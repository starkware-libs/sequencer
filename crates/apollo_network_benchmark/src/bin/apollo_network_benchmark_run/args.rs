use std::collections::HashMap;

use anyhow::Context;
use apollo_network_benchmark::node_args::{Mode, UserArgs};
use clap::Parser;
use serde::Serialize;

pub const STRESS_TEST_NAME: &str = "broadcast-network-stress-test";
pub const METRIC_PORT_BASE: u16 = 2000;
pub const P2P_PORT_BASE: u16 = 10000;

pub fn port_with_offset(base: u16, node_index: u32) -> anyhow::Result<u16> {
    base.checked_add(u16::try_from(node_index).context("Node index too large")?)
        .context("Port overflow")
}

/// Optional `tc`-based traffic shaping applied to each node's network egress.
#[derive(Parser, Debug, Clone, Serialize)]
pub struct NetworkControls {
    /// Min latency to use when gating the network in milliseconds
    #[arg(long)]
    pub latency: Option<u32>,

    /// Max throughput to use when gating the network in KB/s
    #[arg(long)]
    pub throughput: Option<u32>,
}

/// Per-pod CPU/memory requests and limits for cluster deployments.
#[derive(Parser, Debug, Serialize)]
pub struct ResourceLimits {
    /// CPU requests for each network stress test pod
    #[arg(long, default_value = "7500m")]
    pub cpu_requests: String,

    /// Memory requests for each network stress test pod
    #[arg(long, default_value = "10Gi")]
    pub memory_requests: String,

    /// CPU limit for each network stress test pod
    #[arg(long, default_value = "7500m")]
    pub cpu_limits: String,

    /// Memory limit for each network stress test pod
    #[arg(long, default_value = "10Gi")]
    pub memory_limits: String,
}

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
    network_controls: &NetworkControls,
) -> anyhow::Result<Vec<(String, String)>> {
    let mut pairs = vec![];

    if let Some(id) = id {
        pairs.push(("ID".to_string(), id.to_string()));
    }

    pairs.extend_from_slice(&[
        ("METRIC_PORT".to_string(), metric_port.to_string()),
        ("P2P_PORT".to_string(), p2p_port.to_string()),
        ("BOOTSTRAP".to_string(), bootstrap_nodes.join(",")),
        ("TIMEOUT_SECONDS".to_string(), args.user.timeout_seconds.to_string()),
        ("VERBOSITY".to_string(), args.user.verbosity.to_string()),
        ("BUFFER_SIZE".to_string(), args.user.buffer_size.to_string()),
        ("MESSAGE_SIZE_BYTES".to_string(), args.user.message_size_bytes.to_string()),
        ("HEARTBEAT_MILLIS".to_string(), args.user.heartbeat_millis.to_string()),
        ("MODE".to_string(), args.user.mode.to_string()),
        ("NETWORK_PROTOCOL".to_string(), args.user.network_protocol.to_string()),
        ("ROUND_DURATION_SECONDS".to_string(), args.user.round_duration_seconds.to_string()),
        ("NUM_NODES".to_string(), args.num_nodes.to_string()),
    ]);

    // BROADCASTER is only meaningful in OneBroadcast mode and the node binary requires it
    // there via `required_if_eq("mode", "one")`. Setting it unconditionally would leak a
    // confusing `num_nodes - 1` default into env snapshots for the other modes.
    if args.user.mode == Mode::OneBroadcast {
        let broadcaster_id = match args.user.broadcaster {
            Some(broadcaster) => broadcaster,
            None => {
                u64::from(args.num_nodes.checked_sub(1).context("num_nodes must be at least 1")?)
            }
        };
        // Reject ids outside the spawned node range — otherwise no node ever broadcasts
        // and the benchmark silently runs to timeout with empty metrics.
        anyhow::ensure!(
            broadcaster_id < u64::from(args.num_nodes),
            "broadcaster id {broadcaster_id} must be < num_nodes ({})",
            args.num_nodes,
        );
        pairs.push(("BROADCASTER".to_string(), broadcaster_id.to_string()));
    }

    if let Some(latency) = network_controls.latency {
        pairs.push(("LATENCY".to_string(), latency.to_string()));
    }
    if let Some(throughput) = network_controls.throughput {
        pairs.push(("THROUGHPUT".to_string(), throughput.to_string()));
    }

    Ok(pairs)
}

/// Wraps `get_env_var_pairs` into the K8s container env format:
/// `[{"name": "FOO", "value": "bar"}, ...]`
pub fn get_k8s_env_vars(
    id: Option<u32>,
    metric_port: u16,
    p2p_port: u16,
    bootstrap_nodes: &[String],
    args: &SharedArgs,
    network_controls: &NetworkControls,
) -> anyhow::Result<Vec<HashMap<String, String>>> {
    Ok(get_env_var_pairs(id, metric_port, p2p_port, bootstrap_nodes, args, network_controls)?
        .into_iter()
        .map(|(name, value)| {
            HashMap::from([("name".to_string(), name), ("value".to_string(), value)])
        })
        .collect())
}
