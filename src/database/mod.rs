pub mod models;
pub mod reads;
pub mod setup;
pub mod writes;

use rusqlite::{Connection, Result};

/// The core Database engine for LayeredFS
pub struct LayeredDb {
    pub conn: Connection,
}

impl LayeredDb {
    // Initialize the database connection
    pub fn new() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Ok(LayeredDb { conn })
    }
}
