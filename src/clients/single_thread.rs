use color_eyre::Result;
use fnv::FnvHashMap;

use std::io::Write;

use crate::client::Client;
use crate::IncomingTransaction;

/// A single threaded syncronous implementation of clients
///
/// Each csv row is processed exactly in order and processing of
/// the next row won't start until the previous is complete
#[derive(Default)]
pub struct SyncClients(FnvHashMap<u16, Client>);

impl SyncClients {
    /// Takes an iterator of incoming transactions them and processes them sequentially,
    /// reconciling any disputes that occur throughout
    pub fn process(
        &mut self,
        iter: impl Iterator<Item = std::result::Result<IncomingTransaction, csv::Error>>,
    ) -> Result<()> {
        for trx in iter {
            self.publish_transaction(trx?).ok();
        }
        Ok(())
    }

    /// Outputs the current state of the clients to the provided writer
    pub fn output(&self, writer: impl Write) -> Result<()> {
        let mut writer = csv::Writer::from_writer(writer);
        for client in self.0.values() {
            writer.serialize(client)?;
        }
        writer.flush()?;
        Ok(())
    }

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
        client.publish_transaction(tx, ty, amount)
    }
}
