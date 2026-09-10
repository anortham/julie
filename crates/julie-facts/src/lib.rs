//! `facts.sqlite`: the one durable database per checkout.
//!
//! Facts are keyed by blob hash, extracted once per blob, and never updated.
//! Only `paths` changes after insert. This crate knows nothing about
//! workspaces, handlers, or Tantivy.

pub mod insert;
pub mod reader;
pub mod rows;
pub mod schema;
pub mod store;
pub mod version;
pub mod writer;

pub use reader::FactsReader;
pub use store::{FactsStore, Opened};
pub use writer::{Applied, Extractor, FactsWriter, PathChange};

#[cfg(test)]
mod tests;
