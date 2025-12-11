use std::fmt::Display;

use clap::{Parser, ValueEnum};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, ValueEnum, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetworkProtocol {
    /// Use gossipsub for broadcasting (default)
    #[value(name = "gossipsub")]
    Gossipsub,
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

    /// The network protocol to use for communication (default: gossipsub)
    #[arg(long, env, default_value = "gossipsub")]
    pub network_protocol: NetworkProtocol,

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
