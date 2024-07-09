use papyrus_config::dumping::SerializeConfig;
use starknet_mempool_node::config::{MempoolNodeConfig, DEFAULT_CONFIG_PATH};

/// Updates the default config file by:
/// cargo run --bin mempool_dump_config -q
fn main() {
    MempoolNodeConfig::default()
        .dump_to_file(&vec![], DEFAULT_CONFIG_PATH)
        .expect("dump to file error");
}
