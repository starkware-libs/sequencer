use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[command(version, about, long_about = None)]
/// Arguments from the runner, not meant to be set by the user.
pub struct RunnerArgs {}

#[derive(Parser, Debug, Clone)]
#[command(version, about, long_about = None)]
/// Arguments from the user.
pub struct UserArgs {
    /// Set the verbosity level of the logger, the higher the more verbose
    #[arg(short, long, env, default_value = "2")]
    pub verbosity: u8,
}

#[derive(Parser, Debug, Clone)]
#[command(version, about, long_about = None)]
pub struct NodeArgs {
    #[command(flatten)]
    pub runner: RunnerArgs,
    #[command(flatten)]
    pub user: UserArgs,
}
