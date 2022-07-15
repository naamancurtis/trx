//! The high level entrypoint to the crate

use std::fmt;
use std::io::Write;

use color_eyre::Result;
use fnv::FnvHashMap;
use serde::{Deserialize, Serialize};

use crate::amount::Amount;
use crate::client::Client;
use crate::transaction_state::TransactionType;

#[derive(Deserialize, Serialize)]
pub struct IncomingTransaction {
    #[serde(rename = "type")]
    pub ty: TransactionType,
    pub client: u16,
    pub tx: u32,
    pub amount: Option<Amount>,
}

impl fmt::Debug for IncomingTransaction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Transaction")
            .field("type", &self.ty)
            .field("client", &self.client)
            .field("tx", &self.tx)
            .finish()
    }
}

#[derive(Default)]
pub struct Clients(FnvHashMap<u16, Client>);

impl Clients {
    /// Takes an iterator of incoming transactions them and processes them sequentially,
    /// reconciling any disputes that occur throughout
    pub fn process(
        &mut self,
        iter: impl Iterator<Item = std::result::Result<IncomingTransaction, csv::Error>>,
    ) -> Result<()> {
        for trx in iter {
            let IncomingTransaction {
                ty,
                client,
                tx,
                amount,
            } = trx?;
            self.publish_transaction(client, tx, ty, amount).ok();
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
        client_id: u16,
        transaction_id: u32,
        transaction_type: TransactionType,
        amount: Option<Amount>,
    ) -> Result<()> {
        let client = self
            .0
            .entry(client_id)
            .or_insert_with(|| Client::new(client_id));
        client.publish_transaction(transaction_id, transaction_type, amount)
    }
}
