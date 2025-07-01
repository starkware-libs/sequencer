use clap::Parser;
mod converters;
mod utils;

#[derive(Parser, Debug, Clone)]
#[command(version, about, long_about = None)]
struct Args {
    /// ID for Prometheus logging
    #[arg(short, long)]
    id: usize,

    /// The port to run the Prometheus metrics server on
    #[arg(long, default_value_t = 2000)]
    metric_port: u16,

    /// The port to run the P2P network on
    #[arg(short, long, default_value_t = 10000)]
    p2p_port: u16,

    /// The address to the bootstrap peer
    #[arg(short, long, default_value_t = String::new())]
    bootstrap: String,

    /// Set the verbosity level of the logger
    #[arg(short, long, default_value_t = 0)]
    verbosity: u8,

    /// Buffer size for the broadcast topic
    #[arg(short, long, default_value_t = 1000)]
    buffer_size: usize,

    /// Size of StressTestMessage
    #[arg(short, long, default_value_t = 1000)]
    message_size: usize,

    /// Number of messages to send
    #[arg(short, long, default_value_t = 6000)]
    num_messages: u32,

    /// The time to sleep between broadcasts of StressTestMessage in milliseconds
    #[arg(long, default_value_t = 1_000)]
    heartbeat: u64,

    /// Maximum duration in seconds to run the node for
    #[arg(short, long, default_value_t = 3_600)]
    timeout: u64,
}
fn main() {}
