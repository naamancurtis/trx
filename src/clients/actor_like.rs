use async_trait::async_trait;
use color_eyre::{eyre::eyre, Result};
use fnv::FnvHashMap;
use futures::future::join_all;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio::task::{self, JoinHandle};
use tracing::{error, warn};

use std::io::Write;
use std::mem;

use crate::client::Client;
use crate::transaction::IncomingTransaction;

use super::AsyncClients;

/// An Aysnc implementation of Clients
///
/// Behind the scenes it creates a [`tokio::task`] for each client. Any csv row associated
/// with that client is then sent to the task through a channel.
///
/// This is a lightweight simplified interpretation of the `actor` pattern.
///
/// In reality given the lack of compute required by each task coupled with the lack of network
/// traffic, we won't really see a benefit to this approach. However should those things be
/// introduced we should quickly start to see the benefits.
#[derive(Default)]
pub struct Clients {
    join_handles: Vec<JoinHandle<Client>>,
    channels: FnvHashMap<u16, UnboundedSender<IncomingTransaction>>,
}

#[async_trait]
impl AsyncClients for Clients {
    async fn publish_transaction(&mut self, transaction: IncomingTransaction) -> Result<()> {
        let client_id = transaction.client;
        if let Some(c) = self.channels.get(&client_id) {
            c.send(transaction).ok();
            return Ok(());
        }
        let (tx, mut rx) = unbounded_channel();
        let cli = Client::new(client_id);
        let handle = task::spawn(async move {
            let mut cli = cli;
            'process: while let Some(trx) = rx.recv().await {
                let IncomingTransaction { ty, tx, amount, .. } = trx;
                if let Err(e) = cli.publish_transaction(tx, ty, amount) {
                    warn!(error = %e, "stopping processing for client {}", cli.id);
                    // If we have an error we have either had:
                    // - An unexpected, unrecoverable error
                    // - An account freeze
                    // In either scenario, we can no longr proceed to process
                    // this client
                    break 'process;
                }
            }
            cli
        });
        self.channels.insert(client_id, tx);
        self.join_handles.push(handle);
        if let Some(c) = self.channels.get(&client_id) {
            c.send(transaction).ok();
        } else {
            error!(
                "somehow failed to add the channel and join handle for client {}",
                client_id
            );
            return Err(eyre!("failed to create resources needed for client"));
        }
        Ok(())
    }

    /// Outputs the current state of the clients to the provided writer
    async fn output(mut self, writer: impl Write + Send + Sync) -> Result<()> {
        // Close the channels
        self.channels.clear();

        // Finish up the tasks
        let clients = join_all(mem::take(&mut self.join_handles)).await;
        let mut writer = csv::Writer::from_writer(writer);
        for client in clients {
            writer.serialize(client?)?;
        }
        writer.flush()?;
        Ok(())
    }
}
