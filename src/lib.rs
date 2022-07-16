//! # Trx
//!
//! This library is focused on exposing a very lightweight api through which to drive
//! a simple _toy_ transaction/payments engine.
//!
//! The main entry point for library is to implement one of the `*-Engine` traits - [`SyncEngine`] or
//! [`AsyncEngine`].
//! These engines require an iterator over a [`IncomingTransaction`] to process and each have their own
//! style of how they distribute the workload.
//!
//! This library also provides a number of already implemented engines - see [`engines`] for
//! examples
//!
//! ## Examples
//!
//! ### Single-Threaded
//!
//! ```
//! use lib::{SyncEngine, run_sync};
//! use lib::engines::BasicEngine;
//! use std::path::PathBuf;
//!
//! let path = PathBuf::from("./test_assets/simple/spec.csv");
//! let engine: BasicEngine = Default::default();
//!
//! run_sync(path, engine).unwrap();
//! ```
//!
//! ### Multi-Threaded
//!
//! ```
//! use lib::{SyncEngine, run_sync};
//! use lib::engines::StreamLikeEngine;
//! use std::path::PathBuf;
//!
//! let path = PathBuf::from("./test_assets/simple/spec.csv");
//! let engine: StreamLikeEngine = Default::default();
//!
//! run_sync(path, engine).unwrap();
//! ```
//!
//! ### Async
//!
//! ```rust
//! use lib::{AsyncEngine, run_async};
//! use lib::engines::ActorLikeEngine;
//! use std::path::PathBuf;
//!
//! #[tokio::main]
//! async fn main() {
//!     let path = PathBuf::from("./test_assets/simple/spec.csv");
//!     let engine: ActorLikeEngine = Default::default();
//!     run_async(path, engine).await.unwrap();
//! }
//! ```
//!
//! [`IncomingTransaction`]: crate::transaction::IncomingTransaction

pub mod amount;
pub mod engines;
pub mod storage;
pub mod transaction;

#[doc(inline)]
pub use amount::Amount;

#[doc(inline)]
pub use storage::ClientStorage;

#[doc(inline)]
#[cfg(feature = "async")]
pub use engines::AsyncEngine;

#[doc(inline)]
#[cfg(feature = "sync")]
pub use engines::SyncEngine;

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
pub fn run_sync(path: PathBuf, mut engine: impl SyncEngine) -> color_eyre::Result<()> {
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .trim(csv::Trim::All)
        .from_path(path)?;
    let iter = reader.deserialize::<transaction::IncomingTransaction>();
    engine.process(iter)?;
    let mut writer = csv::WriterBuilder::new()
        .from_writer(std::io::stdout())
        .into_inner()?;
    engine.output(&mut writer)?;
    Ok(())
}

/// A helper function to read a csv file from the provided path, process it asynchronously and
/// write the result to `stdout`
#[cfg(feature = "async")]
pub async fn run_async(
    path: PathBuf,
    mut engine: impl AsyncEngine + Send + Sync,
) -> color_eyre::Result<()> {
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .trim(csv::Trim::All)
        .from_path(path)?;
    let iter = reader.deserialize::<transaction::IncomingTransaction>();
    engine.process(iter).await?;
    let mut writer = csv::WriterBuilder::new()
        .from_writer(std::io::stdout())
        .into_inner()?;
    engine.output(&mut writer).await?;
    Ok(())
}
