//! Tests for the deep_dive tool — progressive-depth, kind-aware symbol context
//!
//! Two test layers:
//! 1. Formatting tests: construct SymbolContext in memory, verify output strings
//! 2. Data layer tests: create temp SQLite, store symbols + relationships, test queries

#[cfg(test)]
mod deserialization_tests;

#[cfg(test)]
mod formatting_tests;

#[cfg(test)]
mod data_tests;
