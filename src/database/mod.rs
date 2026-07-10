pub mod models;

use chrono::Utc;
use models::{Block, Inode, Layer};
use rusqlite::{params, Connection, Result};

/// The core Database engine for LayeredFS
pub struct LayeredDb {
    conn: Connection,
}

impl LayeredDb {
    // 1. Initialize the database connection
    pub fn new() -> Result<Self> {
        // We will keep using in-memory for now so we don't leave trash files while testing.
        // Later, we will change this to Connection::open("layeredfs.db")
        let conn = Connection::open_in_memory()?;
        Ok(LayeredDb { conn })
    }

    // Create the tables based on your architecture document
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
                inode_id INTEGER PRIMARY KEY,
                layer_id TEXT,
                parent_id INTEGER,
                name TEXT,
                file_type TEXT,
                size INTEGER,
                permissions INTEGER,
                mtime TEXT,
                semantic_summary TEXT
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
        // Create tables
        self.init_schema()?;

        let current_time = Utc::now().to_rfc3339();

        // Seed Base Layer
        let base_layer = Layer {
            layer_id: "base".to_string(),
            parent_layer_id: None,
            priority: 0,
            is_readonly: true,
            created_at: current_time.clone(),
        };
        self.insert_layer(&base_layer)?;

        // Seed Root Inode
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

        // --- Seed hello.txt (Inode 2) ---

        let file_text = b"Hello from SQLite!\n"; // The 'b' prefix makes it a byte array
        let current_time = Utc::now().to_rfc3339();

        let hello_inode = Inode {
            inode_id: 2,
            layer_id: "base".to_string(),
            parent_id: 1, // It lives inside the root folder
            name: "hello.txt".to_string(),
            file_type: "file".to_string(),
            size: file_text.len() as u64, // The size matches the byte length exactly
            permissions: 0o644,           // Standard rw-r--r-- file permissions
            mtime: current_time,
            semantic_summary: Some("A friendly greeting test file.".to_string()),
        };
        self.insert_inode(&hello_inode)?;

        // Insert the actual data into block 0
        let hello_block = Block {
            inode_id: 2,
            layer_id: "base".to_string(),
            block_index: 0,
            data: file_text.to_vec(),
        };
        self.insert_block(&hello_block)?;

        Ok(())
    }

    // 3. Insert a layer (This clears your "never constructed" warning!)
    pub fn insert_layer(&self, layer: &Layer) -> Result<()> {
        // We use the params! macro to safely bind variables and prevent SQL injection
        self.conn.execute(
            "INSERT INTO layers (layer_id, parent_layer_id, priority, is_readonly, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                layer.layer_id,
                layer.parent_layer_id,
                layer.priority,
                layer.is_readonly,
                layer.created_at,
            ],
        )?;
        Ok(())
    }

    // 4. Insert an Inode
    pub fn insert_inode(&self, inode: &Inode) -> Result<()> {
        self.conn.execute(
            "INSERT INTO inodes (inode_id, layer_id, parent_id, name, file_type, size, permissions, mtime, semantic_summary)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                inode.inode_id,
                inode.layer_id,
                inode.parent_id,
                inode.name,
                inode.file_type,
                inode.size,
                inode.permissions,
                inode.mtime,
                inode.semantic_summary,
            ],
        )?;
        Ok(())
    }

    // 5. Fetch an Inode by its ID
    pub fn get_inode(&self, inode_id: u64) -> Result<Inode> {
        self.conn.query_row(
            "SELECT inode_id, layer_id, parent_id, name, file_type, size, permissions, mtime, semantic_summary
             FROM inodes WHERE inode_id = ?1",
            params![inode_id],
            |row| {
                Ok(Inode {
                    inode_id: row.get(0)?,
                    layer_id: row.get(1)?,
                    parent_id: row.get(2)?,
                    name: row.get(3)?,
                    file_type: row.get(4)?,
                    size: row.get(5)?,
                    permissions: row.get(6)?, // SQLite stores this as INTEGER, Rust maps it to u16
                    mtime: row.get(7)?,
                    semantic_summary: row.get(8)?,
                })
            },
        )
    }

    // Fetch an Inode by its Parent ID and Name (Used for `lookup`)
    pub fn get_inode_by_name(&self, parent_id: u64, name: &str) -> Result<Inode> {
        self.conn.query_row(
            "SELECT inode_id, layer_id, parent_id, name, file_type, size, permissions, mtime, semantic_summary
             FROM inodes WHERE parent_id = ?1 AND name = ?2",
            params![parent_id, name],
            |row| {
                Ok(Inode {
                    inode_id: row.get(0)?,
                    layer_id: row.get(1)?,
                    parent_id: row.get(2)?,
                    name: row.get(3)?,
                    file_type: row.get(4)?,
                    size: row.get(5)?,
                    permissions: row.get(6)?,
                    mtime: row.get(7)?,
                    semantic_summary: row.get(8)?,
                })
            },
        )
    }

    // 6. Fetch children of a directory
    pub fn get_directory_children(&self, parent_id: u64) -> Result<Vec<Inode>> {
        // We use inode_id != parent_id to prevent the root folder (which is its own parent)
        // from being returned as a child of itself.
        let mut stmt = self.conn.prepare(
            "SELECT inode_id, layer_id, parent_id, name, file_type, size, permissions, mtime, semantic_summary
             FROM inodes WHERE parent_id = ?1 AND inode_id != ?1"
        )?;

        let inode_iter = stmt.query_map(params![parent_id], |row| {
            Ok(Inode {
                inode_id: row.get(0)?,
                layer_id: row.get(1)?,
                parent_id: row.get(2)?,
                name: row.get(3)?,
                file_type: row.get(4)?,
                size: row.get(5)?,
                permissions: row.get(6)?,
                mtime: row.get(7)?,
                semantic_summary: row.get(8)?,
            })
        })?;

        // Collect the results into a Rust Vector
        let mut children = Vec::new();
        for inode in inode_iter {
            children.push(inode?);
        }

        Ok(children)
    }

    pub fn insert_block(&self, block: &Block) -> Result<()> {
        self.conn.execute(
            "INSERT INTO blocks (inode_id, layer_id, block_index, data)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                block.inode_id,
                block.layer_id,
                block.block_index,
                block.data,
            ],
        )?;
        Ok(())
    }

    pub fn get_block(&self, inode_id: u64, block_index: i64) -> Result<Block> {
        self.conn.query_row(
            "SELECT inode_id, layer_id, block_index, data
             FROM blocks WHERE inode_id = ?1 AND block_index = ?2",
            params![inode_id, block_index],
            |row| {
                Ok(Block {
                    inode_id: row.get(0)?,
                    layer_id: row.get(1)?,
                    block_index: row.get(2)?,
                    data: row.get(3)?, // SQLite BLOB maps directly to Rust Vec<u8>
                })
            },
        )
    }
}
