use std::fmt::Display;

use clap::{Parser, ValueEnum};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, ValueEnum, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    /// All nodes broadcast messages
    #[value(name = "all")]
    AllBroadcast,
    /// Only the node specified by --broadcaster broadcasts messages
    #[value(name = "one")]
    OneBroadcast,
}

#[derive(Debug, Clone, ValueEnum, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetworkProtocol {
    /// Use gossipsub for broadcasting (default)
    #[value(name = "gossipsub")]
    Gossipsub,
    /// Use SQMR for point-to-point communication
    #[value(name = "sqmr")]
    Sqmr,
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
/// Arguments from the runner, not meant to be set by the user.
pub struct RunnerArgs {
    /// ID for Prometheus logging
    #[arg(short, long, env)]
    pub id: u64,

    /// The port to run the Prometheus metrics server on
    #[arg(long, env)]
    pub metric_port: u16,

    /// The port to run the P2P network on
    #[arg(short, env, long)]
    pub p2p_port: u16,

    /// The addresses of the bootstrap peers (can specify multiple)
    #[arg(long, env, value_delimiter = ',')]
    pub bootstrap: Vec<String>,
}

#[derive(Parser, Debug, Clone)]
#[command(version, about, long_about = None)]
/// Arguments from the user.
pub struct UserArgs {
    /// Set the verbosity level of the logger, the higher the more verbose
    #[arg(short, long, env, default_value = "2")]
    pub verbosity: u8,

    /// Buffer size for the broadcast topic
    #[arg(long, env, default_value = "100000")]
    pub buffer_size: usize,

    /// The mode to use for the stress test.
    #[arg(long, env, default_value = "all")]
    pub mode: Mode,

    /// The network protocol to use for communication (default: gossipsub)
    #[arg(long, env, default_value = "gossipsub")]
    pub network_protocol: NetworkProtocol,

    /// Which node ID should do the broadcasting - for OneBroadcast mode
    #[arg(long, env, required_if_eq("mode", "one"))]
    pub broadcaster: Option<u64>,

    /// Size of StressTestMessage
    #[arg(long, env, default_value = "1024")]
    pub message_size_bytes: usize,

    /// The time to sleep between broadcasts of StressTestMessage in milliseconds
    #[arg(long, env, default_value = "1000")]
    pub heartbeat_millis: u64,

    /// The timeout in seconds for the node.
    /// When the node runs for longer than this, it will be killed.
    #[arg(long, env, default_value = "4000")]
    pub timeout: u64,
}

#[derive(Parser, Debug, Clone)]
#[command(version, about, long_about = None)]
pub struct NodeArgs {
    #[command(flatten)]
    pub runner: RunnerArgs,
    #[command(flatten)]
    pub user: UserArgs,
}
