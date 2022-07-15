//! # Trx
//!
//! This library is focused on exposing a very lightweight api through which to drive
//! a simple transaction engine.
//!
//! The main entry point for library is the [`Clients`] struct which requires an iterator of
//! [`IncomingTransaction`] to then process
//!
//! ```
//! use lib::{Clients, IncomingTransaction};
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

pub mod amount;
pub mod client;
pub mod clients;
pub mod transaction_state;

pub use clients::{Clients, IncomingTransaction};
pub use transaction_state::{Transaction, TransactionType};

/// A re-export of [`clap::Parser`] for ease of use
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
