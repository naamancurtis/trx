use clap::Parser;
use color_eyre::Result;
use csv::{ReaderBuilder, Trim};
use lib::clients::AsyncClients;

use std::io;

use lib::{Cli, IncomingTransaction, SyncClients};

// fn main() -> Result<()> {
//     color_eyre::install()?;
//     let args = Cli::parse();
//     let mut reader = ReaderBuilder::new().trim(Trim::All).from_path(args.path)?;
//     let mut clients: SyncClients = Default::default();
//     let iter = reader.deserialize::<IncomingTransaction>();
//     clients.process(iter)?;
//     clients.output(io::stdout())?;
//     Ok(())
// }

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    let args = Cli::parse();
    let mut reader = ReaderBuilder::new().trim(Trim::All).from_path(args.path)?;
    let mut clients: AsyncClients = Default::default();
    let iter = reader.deserialize::<IncomingTransaction>();
    clients.process(iter)?;
    clients.output(io::stdout()).await?;
    Ok(())
}
