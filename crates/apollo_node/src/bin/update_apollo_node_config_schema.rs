use std::collections::BTreeSet;

use apollo_config::dumping::SerializeConfig;
use apollo_config::ParamPath;
use apollo_infra_utils::dumping::serialize_to_file;
use apollo_node_config::node_config::{
    SequencerNodeConfig,
    CONFIG_POINTERS,
    CONFIG_SECRETS_SCHEMA_PATH,
};

/// Derives the private-parameter set from the config dump: all private non-pointer parameters, plus
/// the pointer targets of any private pointer members. This is the source of truth the committed
/// secrets schema is generated from; `private_parameters()` reads back the committed file.
fn private_parameters_from_config_dump() -> BTreeSet<ParamPath> {
    let dumped_config = SequencerNodeConfig::default().dump();

    let mut private_values = BTreeSet::new();
    for (param_path, ser_param) in dumped_config.into_iter() {
        if !ser_param.is_private() {
            continue;
        }
        let mut included_as_a_pointer = false;
        for ((pointer_target_param_path, _ser_param), pointing_params) in CONFIG_POINTERS.iter() {
            if pointing_params.contains(&param_path) {
                private_values.insert(pointer_target_param_path.clone());
                included_as_a_pointer = true;
            }
        }
        if !included_as_a_pointer {
            private_values.insert(param_path);
        }
    }
    private_values
}

/// Updates the committed apollo node secrets schema (`CONFIG_SECRETS_SCHEMA_PATH`), which is the
/// serialized private-parameter set derived from the config.
fn main() {
    serialize_to_file(&private_parameters_from_config_dump(), CONFIG_SECRETS_SCHEMA_PATH);
}
