use apollo_config::dumping::SerializeConfig;
use apollo_infra_utils::dumping::serialize_to_file;
use apollo_node::config::config_utils::private_parameters;
use apollo_node::config::node_config::{
    SequencerNodeConfig,
    CONFIG_NON_POINTERS_WHITELIST,
    CONFIG_POINTERS,
    CONFIG_SCHEMA_PATH,
    CONFIG_SECRETS_SCHEMA_PATH,
};

/// Updates the apollo node config schema.
fn main() {
    SequencerNodeConfig::default()
        .dump_to_file(&CONFIG_POINTERS, &CONFIG_NON_POINTERS_WHITELIST, CONFIG_SCHEMA_PATH)
        .expect("dump to file error");

    serialize_to_file(private_parameters(), CONFIG_SECRETS_SCHEMA_PATH);
}
