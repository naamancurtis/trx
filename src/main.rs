use clap::Parser;
use color_eyre::Result;

use lib::clients::synchronous::Clients;
use lib::{run_sync, Cli};

fn main() -> Result<()> {
    color_eyre::install()?;
    let args = Cli::parse();
    run_sync(args.path, Clients::default())?;
    Ok(())
}
