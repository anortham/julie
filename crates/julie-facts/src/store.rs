//! Open or create `facts.sqlite`. Never migrates: a version mismatch is
//! reported and the caller deletes the index directory.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Result, anyhow};
use rusqlite::Connection;

use crate::reader::FactsReader;
use crate::schema;
use crate::version::{FACTS_SCHEMA_VERSION, SEMANTIC_INDEX_ENGINE_VERSION};

pub struct FactsStore {
    path: Option<PathBuf>,
    conn: Connection,
}

pub enum Opened {
    Ready(FactsStore),
    VersionMismatch {
        found_schema: i32,
        found_engine: String,
    },
}

impl FactsStore {
    /// Open `path`, creating the schema in an empty file. An existing file whose
    /// `meta` versions differ is left untouched and reported as a mismatch.
    pub fn open(path: impl AsRef<Path>) -> Result<Opened> {
        let path = path.as_ref().to_path_buf();
        let conn = Connection::open(&path)?;
        conn.busy_timeout(Duration::from_millis(5000))?;
        match schema::read_versions(&conn)? {
            Some((found_schema, found_engine))
                if found_schema != FACTS_SCHEMA_VERSION
                    || found_engine != SEMANTIC_INDEX_ENGINE_VERSION =>
            {
                return Ok(Opened::VersionMismatch {
                    found_schema,
                    found_engine,
                });
            }
            Some(_) => configure(&conn)?,
            None => {
                configure(&conn)?;
                schema::create_schema(&conn)?;
            }
        }
        Ok(Opened::Ready(Self {
            path: Some(path),
            conn,
        }))
    }

    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute("PRAGMA foreign_keys = ON", [])?;
        schema::create_schema(&conn)?;
        Ok(Self { path: None, conn })
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub(crate) fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    pub fn reader(&self) -> FactsReader<'_> {
        FactsReader::new(&self.conn, self.path.as_deref())
    }
}

fn configure(conn: &Connection) -> Result<()> {
    let mode: String = conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))?;
    if !mode.eq_ignore_ascii_case("wal") {
        return Err(anyhow!(
            "Failed to enable WAL mode (got '{mode}'). This filesystem may not support WAL."
        ));
    }
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.execute("PRAGMA foreign_keys = ON", [])?;
    Ok(())
}
