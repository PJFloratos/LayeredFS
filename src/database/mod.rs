pub mod models;
pub mod reads;
pub mod setup;
pub mod writes;

use rusqlite::{Connection, Result};

/// The core Database engine for LayeredFS
pub struct LayeredDb {
    pub conn: Connection,
    pub head_priority: Option<i32>, // Tracks the current time-travel state
}

impl LayeredDb {
    // Initialize the database connection
    pub fn new() -> Result<Self> {
        let conn = Connection::open("layeredfs.db")?;
        Ok(LayeredDb {
            conn,
            head_priority: None, // Default to the newest available layer
        })
    }
}
