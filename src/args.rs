use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Parse statements to Output folder
    Parse,

    /// Upload csv from Output folder to FireFly-III
    Upload(AddArgs),

    /// Clear Input and Output folders
    Clear,
}

#[derive(Debug, Args)]
pub struct AddArgs {
    /// FireFly-III host address to upload to
    pub address: Option<String>,
}
