//! Torn-read defense lives in `FactsWriter::apply`: the bytes it hashes are
//! the bytes it extracts, so a second disk read is not required.
