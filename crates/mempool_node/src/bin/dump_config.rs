use mempool_node::config::{MempoolNodeConfig, DEFAULT_CONFIG_PATH};
use papyrus_config::dumping::SerializeConfig;

/// Updates the default config file by:
/// cargo run --bin dump_config -q
fn main() {
    MempoolNodeConfig::default()
        .dump_to_file(&vec![], DEFAULT_CONFIG_PATH)
        .expect("dump to file error");
}
