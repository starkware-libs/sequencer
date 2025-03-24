use starknet_infra_utils::dumping::serialize_to_file;
use starknet_sequencer_deployments::deployment_definitions::DEPLOYMENTS;

// TODO(Tsabary): bundle deployment and its preset path together, and create a list of all of these
// pairs. Then in the test, iterate over them and test each one.

/// Creates the deployment json file.
fn main() {
    for deployment_fn in DEPLOYMENTS {
        let deployment_preset = deployment_fn();
        serialize_to_file(
            deployment_preset.get_deployment(),
            deployment_preset.get_dump_file_path(),
        );
    }
}
