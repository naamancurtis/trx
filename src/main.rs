use clap::Parser;
use color_eyre::Result;

use lib::engines::BasicEngine;
use lib::{run_sync, Cli};

fn main() -> Result<()> {
    color_eyre::install()?;
    let args = Cli::parse();
    run_sync(args.path, BasicEngine::default())?;
    Ok(())
}
