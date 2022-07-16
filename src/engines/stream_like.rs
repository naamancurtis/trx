//! An implementation that runs across multiple threads mimicing a _stream-like_ processing model.
//!
//! Each thread holds a distinct number of clients. The number of threads spun up is proportaional
//! to the number of cpus running the process _identified via [`num_cpus::get`]_.
//!
//! For each incoming transaction, it's client id is identified and _"hashed"_ to
//! identify which thread the transaction should be sent to. Each thread processes
//! its transactions using the [`BasicEngine`] implementation. In this manner you could
//! visualize each `thread` representing a `partition` of a Kafka topic. With the task
//! running in the thread acting as the `consumer`.
//!
//! Similar to [`BasicEngine`], the overall ordering of transactions is maintained, however
//! the workload is distributed over multiple threads.
//!
//! # Examples
//!
//! ```
//! use lib::SyncEngine;
//! use lib::transaction::IncomingTransaction;
//! use lib::engines::StreamLikeEngine;
//! use csv::{ReaderBuilder, Trim};
//! use std::path::PathBuf;
//! use std::io;
//!
//! let path = PathBuf::from("./test_assets/simple/spec.csv");
//! let mut reader = ReaderBuilder::new().trim(Trim::All).from_path(path).unwrap();
//! let mut engine: StreamLikeEngine = Default::default();
//! let iter = reader.deserialize::<IncomingTransaction>();
//! engine.process(iter).unwrap();
//! engine.output(io::stdout()).unwrap();
//! ```
//!
//! [`BasicEngine`]: crate::engines::BasicEngine

use color_eyre::Result;
use crossbeam_channel::{unbounded, Sender, TryRecvError};
use tracing::error;

use std::io::Write;
use std::thread::{self, JoinHandle};

use crate::engines::BasicEngine;
use crate::transaction::IncomingTransaction;

use super::SyncEngine;

/// A multi-threaded _stream-like/kafka-like_ implementation
///
/// Each thread runs their own instance of [`BasicEngine`]
pub struct StreamLikeEngine {
    join_handles: Vec<JoinHandle<Result<BasicEngine>>>,
    channels: Vec<Sender<IncomingTransaction>>,
}

impl Default for StreamLikeEngine {
    fn default() -> Self {
        let cpus = num_cpus::get();
        let mut join_handles = Vec::with_capacity(cpus);
        let mut channels = Vec::with_capacity(cpus);
        for _ in 0..cpus {
            let (s, r) = unbounded();
            let handle = thread::spawn(move || {
                let mut client = BasicEngine::default();
                'process: loop {
                    match r.try_recv() {
                        Ok(msg) => {
                            client.publish_transaction(msg)?;
                        }
                        Err(TryRecvError::Empty) => thread::yield_now(),
                        Err(TryRecvError::Disconnected) => break 'process,
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

impl SyncEngine for StreamLikeEngine {
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
