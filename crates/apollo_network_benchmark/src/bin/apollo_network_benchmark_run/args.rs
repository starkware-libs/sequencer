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

pub fn get_arguments(
    id: Option<u32>,
    metric_port: u16,
    p2p_port: u16,
    bootstrap_nodes: &[String],
    args: &SharedArgs,
) -> Vec<(String, String)> {
    let broadcaster =
        args.user.broadcaster.and_then(|b| u32::try_from(b).ok()).unwrap_or(args.num_nodes - 1);

    let mut result = vec![];

    if let Some(id) = id {
        result.push(("--id".to_string(), id.to_string()));
    }

    result.extend_from_slice(&[
        ("--metric-port".to_string(), metric_port.to_string()),
        ("--p2p-port".to_string(), p2p_port.to_string()),
        ("--bootstrap".to_string(), bootstrap_nodes.join(",")),
        ("--timeout".to_string(), args.user.timeout.to_string()),
        ("--verbosity".to_string(), args.user.verbosity.to_string()),
        ("--buffer-size".to_string(), args.user.buffer_size.to_string()),
        ("--message-size-bytes".to_string(), args.user.message_size_bytes.to_string()),
        ("--heartbeat-millis".to_string(), args.user.heartbeat_millis.to_string()),
        ("--mode".to_string(), args.user.mode.to_string()),
        ("--network-protocol".to_string(), args.user.network_protocol.to_string()),
        ("--broadcaster".to_string(), broadcaster.to_string()),
        ("--round-duration-seconds".to_string(), args.user.round_duration_seconds.to_string()),
        (
            "--explore-cool-down-duration-seconds".to_string(),
            args.user.explore_cool_down_duration_seconds.to_string(),
        ),
        (
            "--explore-run-duration-seconds".to_string(),
            args.user.explore_run_duration_seconds.to_string(),
        ),
        (
            "--explore-min-throughput-byte-per-seconds".to_string(),
            args.user.explore_min_throughput_byte_per_seconds.to_string(),
        ),
        (
            "--explore-min-message-size-bytes".to_string(),
            args.user.explore_min_message_size_bytes.to_string(),
        ),
        ("--num-nodes".to_string(), args.num_nodes.to_string()),
    ]);

    result
}

pub fn get_env_vars(
    id: Option<u32>,
    metric_port: u16,
    p2p_port: u16,
    bootstrap_nodes: &[String],
    args: &SharedArgs,
    latency: Option<u32>,
    throughput: Option<u32>,
) -> Vec<HashMap<String, String>> {
    let arguments = get_arguments(id, metric_port, p2p_port, bootstrap_nodes, args);

    let mut env_vars = vec![];

    // Convert arguments to environment variables
    for (name, value) in arguments {
        let env_name = name[2..].replace("-", "_").to_uppercase();
        let mut env_map = HashMap::new();
        env_map.insert("name".to_string(), env_name);
        env_map.insert("value".to_string(), value);
        env_vars.push(env_map);
    }

    // Add latency and throughput if provided
    if let Some(latency) = latency {
        let mut env_map = HashMap::new();
        env_map.insert("name".to_string(), "LATENCY".to_string());
        env_map.insert("value".to_string(), latency.to_string());
        env_vars.push(env_map);
    }

    if let Some(throughput) = throughput {
        let mut env_map = HashMap::new();
        env_map.insert("name".to_string(), "THROUGHPUT".to_string());
        env_map.insert("value".to_string(), throughput.to_string());
        env_vars.push(env_map);
    }

    env_vars
}
