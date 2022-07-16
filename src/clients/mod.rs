//! Holds the basic proccessing interface for a number of clients who are receiving incoming
//! transactions
//!
//! This module holds two traits [`SyncClients`] and [`AsyncClients`] which sync/async versions of
//! this processing interface.
//!
//! This module also exposes a number of pre-built implementations that can be used out of the box
//!
//! ## Provided Client Flavours
//!
//! 1. A single-threaded synchronous client implementation [`synchronous::Clients`] with in-order processing
//! 2. A multi-threaded client _(think Kafka or AWS Kinesis)_ [`stream_like::Clients`]
//!    _you can think of each thread as a partition_, with the `client_id` being the key dictating
//!    which partition the transaction gets sent to - _allowing us to keep ordering_
//! 3. An async task based client [`actor_like::Clients`], which is something akin to a very lightweight actor
//!    pattern where each client gets their own `actor/task`
//!

#[cfg(feature = "actor_client")]
pub mod actor_like;
#[cfg(feature = "stream_client")]
pub mod stream_like;
#[cfg(feature = "sync_client")]
pub mod synchronous;

use color_eyre::Result;

use std::io::Write;

use crate::transaction::IncomingTransaction;

/// This trait representations the synchronous interface required to process a series of incoming
/// transactions
#[cfg(feature = "sync")]
pub trait SyncClients {
    /// Takes an iterator of incoming transactions them and processes them sequentially,
    /// reconciling any disputes that occur throughout
    ///
    /// # Default Implementation
    ///
    /// The default implementation of this function simply calls [`SyncClients::publish_transaction`]
    /// on every element of the iterator. If either the `Item` yielded by the iterator, or the
    /// publish_transaction call **errors** proccessing will be interupted and this function will
    /// return an error
    fn process(
        &mut self,
        iter: impl Iterator<Item = std::result::Result<IncomingTransaction, csv::Error>>,
    ) -> Result<()> {
        for trx in iter {
            self.publish_transaction(trx?)?;
        }
        Ok(())
    }

    /// The implementation of how an [`IncomingTransaction`] should be processed
    fn publish_transaction(&mut self, transaction: IncomingTransaction) -> Result<()>;
    /// How the results should be outputted once processing is complete
    fn output(self, writer: impl Write) -> Result<()>;
}

/// This trait representations the async interface required to process a series of incoming
/// transactions
#[cfg(feature = "async")]
#[async_trait::async_trait]
pub trait AsyncClients {
    /// Takes an iterator of incoming transactions them and processes them sequentially,
    /// reconciling any disputes that occur throughout
    ///
    /// # Default Implementation
    ///
    /// The default implementation of this function simply calls [`AsyncClients::publish_transaction`]
    /// on every element of the iterator and `await`s the resposne. If either the `Item` yielded by the iterator, or the
    /// publish_transaction call **errors** proccessing will be interupted and this function will
    /// return an error
    async fn process(
        &mut self,
        iter: impl Iterator<Item = std::result::Result<IncomingTransaction, csv::Error>> + Send + Sync,
    ) -> Result<()> {
        for trx in iter {
            self.publish_transaction(trx?).await?;
        }
        Ok(())
    }

    /// The implementation of how an [`IncomingTransaction`] should be processed
    async fn publish_transaction(&mut self, transaction: IncomingTransaction) -> Result<()>;
    /// How the results should be outputted once processing is complete
    async fn output(self, writer: impl Write + Send + Sync) -> Result<()>;
}
