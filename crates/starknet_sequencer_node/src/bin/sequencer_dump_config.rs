use papyrus_config::dumping::SerializeConfig;
use starknet_sequencer_node::config::node_config::{
    SequencerNodeConfig,
    CONFIG_NON_POINTERS_WHITELIST,
    CONFIG_POINTERS,
    DEFAULT_CONFIG_PATH,
};

/// Updates the default config file by:
/// cargo run --bin sequencer_dump_config -q
fn main() {
    SequencerNodeConfig::default()
        .dump_to_file(&CONFIG_POINTERS, &CONFIG_NON_POINTERS_WHITELIST, DEFAULT_CONFIG_PATH)
        .expect("dump to file error");
}
