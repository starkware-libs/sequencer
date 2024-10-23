use std::env::args;

use clap::Parser;
use papyrus_config::ConfigError;

use crate::config::NodeConfig;

/// Split the elements of `input_args` into 2 groups:
/// 1. Those prefixed with `split_args_prefix`
/// 2. Other.
///
/// Presumes input is: program_name (--flag_name value)*
pub fn split_args(input_args: Vec<String>, split_args_prefix: &str) -> (Vec<String>, Vec<String>) {
    input_args[1..].chunks(2).fold(
        (vec![input_args[0].clone()], vec![input_args[0].clone()]),
        |(mut matching_args, mut mismatched_args), input_arg| {
            let (name, value) = (&input_arg[0], &input_arg[1]);
            // String leading `--` for comparison.
            if &name[..split_args_prefix.len()] == split_args_prefix {
                matching_args.push(format!("--{}", &name[split_args_prefix.len()..]));
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
pub fn build_configs<T: Parser + Default>(
    split_args_prefix: &str,
) -> Result<(T, NodeConfig), ConfigError> {
    let input_args = args().collect::<Vec<_>>();
    let (test_input_args, node_input_args) = split_args(input_args, split_args_prefix);

    let mut test_config = T::default();
    test_config.update_from(test_input_args.iter());

    let node_config = NodeConfig::load_and_process(node_input_args);
    if let Err(ConfigError::CommandInput(clap_err)) = node_config {
        clap_err.exit();
    }
    let node_config = node_config?;
    Ok((test_config, node_config))
}
