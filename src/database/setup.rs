use crate::database::models::{Block, Inode, Layer};
use crate::database::LayeredDb;
use chrono::Utc;
use rusqlite::Result;

impl LayeredDb {
    pub fn init_schema(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS layers (
                layer_id TEXT PRIMARY KEY,
                parent_layer_id TEXT,
                priority INTEGER,
                is_readonly BOOLEAN,
                created_at TEXT
            )",
            [],
        )?;

        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS inodes (
                inode_id INTEGER,
                layer_id TEXT,
                parent_id INTEGER,
                name TEXT,
                file_type TEXT,
                size INTEGER,
                permissions INTEGER,
                mtime TEXT,
                semantic_summary TEXT,
                PRIMARY KEY (inode_id, layer_id)
            )",
            [],
        )?;

        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS blocks (
                inode_id INTEGER,
                layer_id TEXT,
                block_index INTEGER,
                data BLOB,
                PRIMARY KEY (inode_id, layer_id, block_index)
            )",
            [],
        )?;

        Ok(())
    }

    pub fn bootstrap_initial_state(&self) -> Result<()> {
        // 1. Create tables if they don't exist
        self.init_schema()?;

        // 2. State Detection: Check if the "base" layer already exists
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM layers WHERE layer_id = 'base'",
            [],
            |row| row.get(0),
        )?;

        if count > 0 {
            // The database is already seeded
            // We gracefully exit the bootstrap process so we don't crash.
            return Ok(());
        }

        // 3. If empty, proceed with the original seeding logic
        let current_time = Utc::now().to_rfc3339();

        let base_layer = Layer {
            layer_id: "base".to_string(),
            parent_layer_id: None,
            priority: 0,
            is_readonly: true,
            created_at: current_time.clone(),
        };
        self.insert_layer(&base_layer)?;

        let active_layer = Layer {
            layer_id: "active".to_string(),
            parent_layer_id: Some("base".to_string()),
            priority: 1,
            is_readonly: false,
            created_at: current_time.clone(),
        };
        self.insert_layer(&active_layer)?;

        let root_inode = Inode {
            inode_id: 1,
            layer_id: "base".to_string(),
            parent_id: 1,
            name: "".to_string(),
            file_type: "dir".to_string(),
            size: 4096,
            permissions: 0o755,
            mtime: current_time,
            semantic_summary: None,
        };
        self.insert_inode(&root_inode)?;

        let file_text = b"Hello from SQLite!\n";
        let current_time = Utc::now().to_rfc3339();
        let hello_inode = Inode {
            inode_id: 2,
            layer_id: "base".to_string(),
            parent_id: 1,
            name: "hello.txt".to_string(),
            file_type: "file".to_string(),
            size: file_text.len() as u64,
            permissions: 0o644,
            mtime: current_time,
            semantic_summary: Some("A friendly greeting test file.".to_string()),
        };
        self.insert_inode(&hello_inode)?;

        let hello_block = Block {
            inode_id: 2,
            layer_id: "base".to_string(),
            block_index: 0,
            data: file_text.to_vec(),
        };
        self.insert_block(&hello_block)?;

        Ok(())
    }

    pub fn wipe_all_data(&self) -> Result<()> {
        // Drop tables in reverse order of dependencies if you ever add foreign keys,
        // but for now any order works!
        self.conn.execute("DROP TABLE IF EXISTS blocks;", [])?;
        self.conn.execute("DROP TABLE IF EXISTS inodes;", [])?;
        self.conn.execute("DROP TABLE IF EXISTS layers;", [])?;
        Ok(())
    }
}
