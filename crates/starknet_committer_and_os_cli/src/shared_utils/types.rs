use clap::Args;

#[derive(Debug, Args)]
pub(crate) struct IoArgs {
    /// File path to input.
    #[clap(long, short = 'i')]
    pub(crate) input_path: String,

    /// File path to output.
    #[clap(long, short = 'o', default_value = "stdout")]
    pub(crate) output_path: String,
}
