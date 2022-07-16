use color_eyre::Result;
use crossbeam_channel::{unbounded, Sender};
use tracing::error;

use std::io::Write;
use std::thread::{self, JoinHandle};

use crate::clients::synchronous::Clients as SynchronousClients;
use crate::transaction::IncomingTransaction;

use super::SyncClients;

/// An Aysnc implementation of clients
///
/// Behind the scenes it creates a [`tokio::task`] for each client. Any csv row associated
/// with that client is then sent to the task through a channel.
pub struct Clients {
    join_handles: Vec<JoinHandle<Result<SynchronousClients>>>,
    channels: Vec<Sender<IncomingTransaction>>,
}

impl Default for Clients {
    fn default() -> Self {
        let cpus = num_cpus::get();
        let mut join_handles = Vec::with_capacity(cpus);
        let mut channels = Vec::with_capacity(cpus);
        for _ in 0..cpus {
            let (s, r) = unbounded();
            let handle = thread::spawn(move || {
                let mut client = SynchronousClients::default();
                'process: loop {
                    match r.recv() {
                        Ok(msg) => {
                            client.publish_transaction(msg)?;
                        }
                        Err(_) => break 'process,
                    };
                }
                Ok(client)
            });
            join_handles.push(handle);
            channels.push(s);
        }
        Self {
            join_handles,
            channels,
        }
    }
}

impl SyncClients for Clients {
    fn publish_transaction(&mut self, transaction: IncomingTransaction) -> Result<()> {
        let client_id = transaction.client;
        let bucket = client_id as usize % self.channels.len();
        self.channels[bucket].send(transaction)?;
        Ok(())
    }

    /// Outputs the current state of the clients to the provided writer
    fn output(mut self, writer: impl Write) -> Result<()> {
        // Close the channels
        self.channels.clear();

        // Finish up the tasks
        let clients = self
            .join_handles
            .into_iter()
            .enumerate()
            .filter_map(|(i, h)| match h.join() {
                Ok(c) => {
                    match c {
                        Ok(c) => {
                            Some(c.clients())
                        }
                        Err(e) => {
                            error!(error = %e, "an error occured on thread {}. the results from it are being ignored as we can't be sure of the validity of them", i);
                            None
                        }
                    }
                },
                Err(e) => {
                    error!(
                        error = ?e, "failed to join thread handle from thread {}, data has been lost",
                        i
                    );
                    None
                }
            })
            .flatten();
        let mut writer = csv::Writer::from_writer(writer);
        for client in clients {
            writer.serialize(client)?;
        }
        writer.flush()?;
        Ok(())
    }
}
