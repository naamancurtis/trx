//! An implementation that runs within a single thread.
//!
//! Each transaction gets processed in the order it is received before moving into the next one
//!
//! # Examples
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

use color_eyre::Result;
use fnv::FnvHashMap;

use std::io::Write;

use crate::client::Client;
use crate::transaction::IncomingTransaction;

use super::SyncClients;

/// A single threaded syncronous implementation of clients
///
/// Each csv row is processed exactly in order and processing of
/// the next row won't start until the previous is complete
#[derive(Default)]
pub struct Clients(FnvHashMap<u16, Client>);

impl SyncClients for Clients {
    fn publish_transaction(
        &mut self,
        IncomingTransaction {
            ty,
            client,
            tx,
            amount,
        }: IncomingTransaction,
    ) -> Result<()> {
        let client = self.0.entry(client).or_insert_with(|| Client::new(client));
        if !client.is_locked() {
            match client.publish_transaction(tx, ty, amount) {
                // TODO - Make this an enum match instead of a string
                Err(e) if !e.to_string().starts_with("[FROZEN_ACCOUNT]") => return Err(e),
                _ => {}
            }
        }
        Ok(())
    }

    /// Outputs the current state of the clients to the provided writer by
    /// serializing the results into a csv format
    fn output(self, writer: impl Write) -> Result<()> {
        let mut writer = csv::Writer::from_writer(writer);
        for client in self.0.values() {
            writer.serialize(client)?;
        }
        writer.flush()?;
        Ok(())
    }
}

impl Clients {
    /// Consumes `self` and returns an iterator over the currently stored [`Client`]
    pub(crate) fn clients(self) -> impl Iterator<Item = Client> {
        self.0.into_values()
    }
}
