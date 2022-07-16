//! Holds various `Clients` implementations.
//!
//! This struct holds all of the Clients that are discovered while processing
//! the incoming data, and also knows how to distribute new records to the [`crate::client::Client`]
//! for processing.
//!
//! ## Flavours
//! This library contains different flavours of clients that follow different patterns
//! 1. Single-Threaded Synchronous [`SyncClients`] with in-order processing
//! 2. Async Task Based [`AsyncClients`], which is something akin to a very lightweight actor
//!    pattern
//!
//! Normally I would put these _flavours_ behind feature flags and ensure the dependencies are
//! optional, however given that this project is going to be run in a certain way I've avoided that
//! for now

mod async_task;
mod single_thread;

pub use async_task::*;
pub use single_thread::*;
