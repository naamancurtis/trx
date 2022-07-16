use color_eyre::Result;
use fnv::FnvHashMap;
use futures::future::join_all;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio::task::{self, JoinHandle};
use tracing::error;

use std::io::Write;

use crate::client::Client;
use crate::IncomingTransaction;

/// An Aysnc implementation of clients
///
/// Behind the scenes it creates a [`tokio::task`] for each client. Any csv row associated
/// with that client is then sent to the task through a channel.
#[derive(Default)]
pub struct AsyncClients {
    join_handles: Vec<JoinHandle<Client>>,
    channels: FnvHashMap<u16, UnboundedSender<IncomingTransaction>>,
}

impl AsyncClients {
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
    pub async fn output(&mut self, writer: impl Write) -> Result<()> {
        // Close the channels
        self.channels.clear();

        // Finish up the tasks
        let clients = join_all(std::mem::take(&mut self.join_handles)).await;
        let mut writer = csv::Writer::from_writer(writer);
        for client in clients {
            writer.serialize(client?)?;
        }
        writer.flush()?;
        Ok(())
    }

    fn publish_transaction(&mut self, transaction: IncomingTransaction) -> Result<()> {
        let client_id = transaction.client;
        if let Some(c) = self.channels.get(&client_id) {
            c.send(transaction)?;
            return Ok(());
        }
        let (tx, mut rx) = unbounded_channel();
        let cli = Client::new(client_id);
        let handle = task::spawn(async move {
            let mut cli = cli;
            while let Some(trx) = rx.recv().await {
                let IncomingTransaction { ty, tx, amount, .. } = trx;
                // Ignore this error as we don't want to stop processing
                match cli.publish_transaction(tx, ty, amount) {
                    Ok(()) => {}
                    Err(e) => error!("{}", e),
                };
            }
            cli
        });
        self.channels.insert(client_id, tx);
        self.join_handles.push(handle);
        Ok(())
    }
}
