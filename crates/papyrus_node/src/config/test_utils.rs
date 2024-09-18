use std::env::args;

use clap::Parser;
use papyrus_config::ConfigError;
use papyrus_network::gossipsub_impl::Topic;

use crate::config::NodeConfig;

// Test arguments passed on the command line are prefixed with `test.<ARG_NAME>`.
const TEST_ARG_PREFIX: &str = "--test.";

/// Split the elements of `input_args` into 2 groups:
/// 1. Those prefixed with "--test."
/// 2. Other.
///
/// Presumes input is: program_name (--flag_name value)*
pub fn split_args(input_args: Vec<String>) -> (Vec<String>, Vec<String>) {
    input_args[1..].chunks(2).fold(
        (vec![input_args[0].clone()], vec![input_args[0].clone()]),
        |(mut matching_args, mut mismatched_args), input_arg| {
            let (name, value) = (&input_arg[0], &input_arg[1]);
            // String leading `--` for comparison.
            if &name[..TEST_ARG_PREFIX.len()] == TEST_ARG_PREFIX {
                matching_args.push(format!("--{}", name[TEST_ARG_PREFIX.len()..].to_string()));
                matching_args.push(value.clone());
            } else {
                mismatched_args.push(name.clone());
                mismatched_args.push(value.clone());
            }
            (matching_args, mismatched_args)
        },
    )
}

/// Build both the node and test configs from the command line arguments.
pub fn build_configs<T: Parser + Default>() -> Result<(T, NodeConfig), ConfigError> {
    let input_args = args().collect::<Vec<_>>();
    let (test_input_args, node_input_args) = split_args(input_args);
    dbg!(&test_input_args, &node_input_args);

    let mut test_config = T::default();
    test_config.update_from(test_input_args.iter());

    let node_config = NodeConfig::load_and_process(node_input_args);
    if let Err(ConfigError::CommandInput(clap_err)) = node_config {
        clap_err.exit();
    }
    let node_config = node_config?;
    Ok((test_config, node_config))
}
