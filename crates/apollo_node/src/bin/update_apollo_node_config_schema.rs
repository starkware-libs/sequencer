use std::collections::BTreeSet;

use apollo_config::dumping::SerializeConfig;
use apollo_config::ParamPath;
use apollo_infra_utils::dumping::serialize_to_file;
use apollo_node_config::node_config::{SequencerNodeConfig, CONFIG_SECRETS_SCHEMA_PATH};

/// Derives the private-parameter set from the config dump: every private parameter. This is the
/// source of truth the committed secrets schema is generated from; `private_parameters()` reads
/// back the committed file.
fn private_parameters_from_config_dump() -> BTreeSet<ParamPath> {
    SequencerNodeConfig::default()
        .dump()
        .into_iter()
        .filter(|(_param_path, ser_param)| ser_param.is_private())
        .map(|(param_path, _ser_param)| param_path)
        .collect()
}

/// Updates the committed apollo node secrets schema (`CONFIG_SECRETS_SCHEMA_PATH`), which is the
/// serialized private-parameter set derived from the config.
fn main() {
    serialize_to_file(&private_parameters_from_config_dump(), CONFIG_SECRETS_SCHEMA_PATH);
}
