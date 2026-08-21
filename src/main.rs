mod args;
mod commands;

use clap::Parser;

use args::{Cli, Commands};
use ass_acc::AppConfig;

const CONFIG_PATH: &str = "Settings/config.toml";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let mut cfg: AppConfig = confy::load_path(CONFIG_PATH)?;

    match cli.command {
        Commands::Parse => commands::parse(&mut cfg, CONFIG_PATH),
        Commands::Upload(args) => match args.address {
            Some(address) => commands::upload(&mut cfg, &address).await,
            None => Err(anyhow::anyhow!("Error: Specify the upload address!")),
        },
        Commands::Clear => commands::clear(&mut cfg),
    }
}
