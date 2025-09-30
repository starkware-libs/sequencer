use std::fmt::Display;
use clap::{Parser, ValueEnum};

#[derive(Debug, Clone, ValueEnum)]
pub enum Mode {
    /// All nodes broadcast messages
    #[value(name = "all")]
    AllBroadcast,
    /// Only the node specified by --broadcaster broadcasts messages
    #[value(name = "one")]
    OneBroadcast,
    /// Nodes take turns broadcasting in round-robin fashion
    #[value(name = "rr")]
    RoundRobin,
    /// Only the node specified by --broadcaster broadcasts messages,
    /// Every explore_run_duration_seconds + explore_cool_down_duration_seconds seconds 
    /// a new combination of MPS and message size is explored.
    /// Increases the throughput with each new trial.
    /// Configurations are filtered by minimum throughput and minimum message size.
    #[value(name = "explore")]
    Explore,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum NetworkProtocol {
    /// Use gossipsub for broadcasting (default)
    #[value(name = "gossipsub")]
    Gossipsub,
    /// Use SQMR for point-to-point communication
    #[value(name = "sqmr")]
    Sqmr,
    /// Use Reversed SQMR where receivers initiate requests to broadcasters
    #[value(name = "reversed-sqmr")]
    ReveresedSqmr,
}

impl Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_possible_value().unwrap().get_name())
    }
}

impl Display for NetworkProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_possible_value().unwrap().get_name())
    }
}

#[derive(Parser, Debug, Clone)]
#[command(version, about, long_about = None)]
pub struct Args {
    /// ID for Prometheus logging
    #[arg(short, long, env)]
    pub id: u64,

    /// Total number of nodes in the network
    #[arg(long, env)]
    pub num_nodes: u64,

    /// The port to run the Prometheus metrics server on
    #[arg(long, env)]
    pub metric_port: u16,

    /// The port to run the P2P network on
    #[arg(short, env, long)]
    pub p2p_port: u16,

    /// The addresses of the bootstrap peers (can specify multiple)
    #[arg(long, env, value_delimiter = ',')]
    pub bootstrap: Vec<String>,

    /// Set the verbosity level of the logger, the higher the more verbose
    #[arg(short, long, env)]
    pub verbosity: u8,

    /// Buffer size for the broadcast topic
    #[arg(long, env)]
    pub buffer_size: usize,

    /// The mode to use for the stress test.
    #[arg(long, env)]
    pub mode: Mode,

    /// The network protocol to use for communication (default: gossipsub)
    #[arg(long, env)]
    pub network_protocol: NetworkProtocol,

    /// Which node ID should do the broadcasting - for OneBroadcast and Explore modes
    #[arg(long, env, required_if_eq_any([("mode", "one"), ("mode", "explore")]))]
    pub broadcaster: Option<u64>,

    /// Duration each node broadcasts before switching (in seconds) - for RoundRobin mode
    #[arg(long, env, required_if_eq("mode", "rr"))]
    pub round_duration_seconds: Option<u64>,

    /// Size of StressTestMessage
    #[arg(long, env, required_if_eq_any([("mode", "one"), ("mode", "all"), ("mode", "rr")]))]
    pub message_size_bytes: Option<usize>,

    /// The time to sleep between broadcasts of StressTestMessage in milliseconds
    #[arg(long, env, required_if_eq_any([("mode", "one"), ("mode", "all"), ("mode", "rr")]))]
    pub heartbeat_millis: Option<u64>,

    /// Cool down duration between configuration changes in seconds - for Explore mode
    #[arg(long, env, required_if_eq("mode", "explore"))]
    pub explore_cool_down_duration_seconds: Option<u64>,

    /// Duration to run each configuration in seconds - for Explore mode
    #[arg(long, env, required_if_eq("mode", "explore"))]
    pub explore_run_duration_seconds: Option<u64>,

    /// Interval for collecting process metrics (CPU, memory) in seconds
    #[arg(long, env)]
    pub system_metrics_interval_seconds: u64,

    /// Minimum throughput in bytes per second - for Explore mode
    #[arg(long, env, required_if_eq("mode", "explore"))]
    pub explore_min_throughput_byte_per_seconds: Option<f64>,

    /// Minimum message size in bytes - for Explore mode
    #[arg(long, env, required_if_eq("mode", "explore"))]
    pub explore_min_message_size_bytes: Option<usize>,

    /// The timeout in seconds for the node.
    /// When the node runs for longer than this, it will be killed.
    #[arg(long, env)]
    pub timeout: u64,
}
