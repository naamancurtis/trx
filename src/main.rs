use clap::Parser;
use color_eyre::Result;
use csv::{ReaderBuilder, Trim};

use std::io;

use lib::{Cli, Clients, IncomingTransaction};

fn main() -> Result<()> {
    color_eyre::install()?;
    let args = Cli::parse();
    let mut reader = ReaderBuilder::new().trim(Trim::All).from_path(args.path)?;
    let mut clients: Clients = Default::default();
    let iter = reader.deserialize::<IncomingTransaction>();
    clients.process(iter)?;
    clients.output(io::stdout())?;
    Ok(())
}
