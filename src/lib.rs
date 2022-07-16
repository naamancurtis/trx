//! # Trx
//!
//! This library is focused on exposing a very lightweight api through which to drive
//! a simple transaction engine.
//!
//! The main entry point for library is to implement one of the `*-Client` traits - [`SyncClients`] or
//! [`AsyncClients`].
//! These clients require an iterator of [`crate::transaction::IncomingTransaction`] to process and each have their own
//! style of how they distribute the workload.
//!
//! This library also provides a number of already implemented clients - see [`clients`] for
//! examples
//!
//! ## Examples
//!
//! ### Single Threaded Synchronous
//!
//! ```
//! use lib::SyncClients;
//! use lib::transaction::IncomingTransaction;
//! use lib::clients::synchronous::Clients;
//! use csv::{ReaderBuilder, Trim};
//! use std::path::PathBuf;
//! use std::io;
//!
//! let path = PathBuf::from("./test_assets/simple/spec.csv");
//! let mut reader = ReaderBuilder::new().trim(Trim::All).from_path(path).unwrap();
//! let mut clients: Clients = Default::default();
//! let iter = reader.deserialize::<IncomingTransaction>();
//! clients.process(iter).unwrap();
//! clients.output(io::stdout()).unwrap();
//! ```
//!
//! ### Multi-Threaded
//!
//! ```
//! use lib::SyncClients;
//! use lib::transaction::IncomingTransaction;
//! use lib::clients::stream_like::Clients;
//! use csv::{ReaderBuilder, Trim};
//! use std::path::PathBuf;
//! use std::io;
//!
//! let path = PathBuf::from("./test_assets/simple/spec.csv");
//! let mut reader = ReaderBuilder::new().trim(Trim::All).from_path(path).unwrap();
//! let mut clients: Clients = Default::default();
//! let iter = reader.deserialize::<IncomingTransaction>();
//! clients.process(iter).unwrap();
//! clients.output(io::stdout()).unwrap();
//! ```
//!
//! ### Async
//!
//! ```rust
//! use lib::AsyncClients;
//! use lib::transaction::IncomingTransaction;
//! use lib::clients::actor_like::Clients;
//!
//! use csv::{ReaderBuilder, Trim};
//! use std::path::PathBuf;
//! use std::io;
//!
//! #[tokio::main]
//! async fn main() {
//!    let path = PathBuf::from("./test_assets/simple/spec.csv");
//!    let mut reader = ReaderBuilder::new().trim(Trim::All).from_path(path).unwrap();
//!    let mut clients: Clients = Default::default();
//!    let iter = reader.deserialize::<IncomingTransaction>();
//!    clients.process(iter).await.unwrap();
//!    clients.output(io::stdout()).await.unwrap();
//! }
//! ```

pub mod amount;
pub mod client;
pub mod clients;
pub mod transaction;

#[doc(inline)]
pub use amount::Amount;

#[doc(inline)]
#[cfg(feature = "async")]
pub use clients::AsyncClients;

#[doc(inline)]
#[cfg(feature = "sync")]
pub use clients::SyncClients;

#[doc(no_inline)]
pub use clap::Parser;

use std::path::PathBuf;

/// A very simple command line argument parser to read a path from the first argument passed to the
/// binary
///
/// ```no_run
/// use lib::{Cli, Parser};
///
/// let args = Cli::parse();
/// println!("Path: {}", args.path.display());
/// ```
#[derive(Parser)]
pub struct Cli {
    #[clap(parse(from_os_str))]
    pub path: PathBuf,
}

/// A helper function to read a csv file from the provided path, process it synchronously and
/// write the result to `stdout`
#[cfg(feature = "sync")]
pub fn run_sync(path: PathBuf, mut clients: impl SyncClients) -> color_eyre::Result<()> {
    let mut reader = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_path(path)?;
    let iter = reader.deserialize::<transaction::IncomingTransaction>();
    clients.process(iter)?;
    let mut writer = csv::WriterBuilder::new()
        .from_writer(std::io::stdout())
        .into_inner()?;
    clients.output(&mut writer)?;
    Ok(())
}

/// A helper function to read a csv file from the provided path, process it asynchronously and
/// write the result to `stdout`
#[cfg(feature = "async")]
pub async fn run_async(
    path: PathBuf,
    mut clients: impl AsyncClients + Send + Sync,
) -> color_eyre::Result<()> {
    let mut reader = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_path(path)?;
    let iter = reader.deserialize::<transaction::IncomingTransaction>();
    clients.process(iter).await?;
    let mut writer = csv::WriterBuilder::new()
        .from_writer(std::io::stdout())
        .into_inner()?;
    clients.output(&mut writer).await?;
    Ok(())
}
