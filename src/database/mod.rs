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

        // Seed Active Layer ---
        let active_layer = Layer {
            layer_id: "active".to_string(),
            parent_layer_id: Some("base".to_string()),
            priority: 1,        // Higher priority shadows the base layer
            is_readonly: false, // This layer accepts writes
            created_at: current_time.clone(),
        };
        self.insert_layer(&active_layer)?;

        // Seed Root Inode (Inode 1)
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

        // Seed hello.txt (Inode 2)
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

    // 5. Fetch an Inode by its ID
    pub fn get_inode(&self, inode_id: u64) -> Result<Inode> {
        let inode = self.conn.query_row(
            "SELECT i.inode_id, i.layer_id, i.parent_id, i.name, i.file_type, i.size, i.permissions, i.mtime, i.semantic_summary
             FROM inodes i
             JOIN layers l ON i.layer_id = l.layer_id
             WHERE i.inode_id = ?1
             ORDER BY l.priority DESC
             LIMIT 1",
            params![inode_id],
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
        )?;

        // If the highest priority layer is a tombstone, it's deleted!
        if inode.file_type == "tombstone" {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        Ok(inode)
    }

    // Fetch an Inode by its Parent ID and Name (Used for `lookup`)
    pub fn get_inode_by_name(&self, parent_id: u64, name: &str) -> Result<Inode> {
        let inode = self.conn.query_row(
            // Order by priority DESC and LIMIT 1 to get the topmost layer's version
            "SELECT i.inode_id, i.layer_id, i.parent_id, i.name, i.file_type, i.size, i.permissions, i.mtime, i.semantic_summary
             FROM inodes i
             JOIN layers l ON i.layer_id = l.layer_id
             WHERE i.parent_id = ?1 AND i.name = ?2
             ORDER BY l.priority DESC
             LIMIT 1",
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
        )?;

        // If the topmost version is a tombstone, pretend it doesn't exist!
        if inode.file_type == "tombstone" {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }

        Ok(inode)
    }

    pub fn get_next_inode_id(&self) -> Result<u64> {
        // COALESCE handles the case where the table is empty by defaulting to 0, then adding 1.
        self.conn.query_row(
            "SELECT COALESCE(MAX(inode_id), 0) + 1 FROM inodes",
            [],
            |row| row.get(0),
        )
    }

    // 6. Fetch children of a directory
    pub fn get_directory_children(&self, parent_id: u64) -> Result<Vec<Inode>> {
        // We use a SQLite Window Function (ROW_NUMBER) to partition by name and sort by priority.
        // This ensures if a file exists in 'active' and 'base', we only look at the 'active' one.
        let mut stmt = self.conn.prepare(
            "SELECT inode_id, layer_id, parent_id, name, file_type, size, permissions, mtime, semantic_summary
             FROM (
                 SELECT i.*, ROW_NUMBER() OVER (PARTITION BY i.name ORDER BY l.priority DESC) as rn
                 FROM inodes i
                 JOIN layers l ON i.layer_id = l.layer_id
                 WHERE i.parent_id = ?1 AND i.inode_id != ?1
             )
             WHERE rn = 1 AND file_type != 'tombstone'"
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

    pub fn update_inode(&self, inode: &Inode) -> Result<()> {
        self.conn.execute(
            "UPDATE inodes SET size = ?1, permissions = ?2, mtime = ?3
             WHERE inode_id = ?4 AND layer_id = ?5",
            params![
                inode.size,
                inode.permissions,
                inode.mtime,
                inode.inode_id,
                inode.layer_id
            ],
        )?;
        Ok(())
    }

    pub fn upsert_block(&self, block: &Block) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO blocks (inode_id, layer_id, block_index, data)
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

    // The Copy-on-Write Engine ---
    pub fn copy_on_write(&self, inode_id: u64) -> Result<Inode> {
        let mut inode = self.get_inode(inode_id)?;

        // 1. If it's already in the active layer, it's safe to modify.
        if inode.layer_id == "active" {
            return Ok(inode);
        }

        // 2. If it's in the base layer, copy it up to the active layer.
        inode.layer_id = "active".to_string();
        inode.mtime = chrono::Utc::now().to_rfc3339();
        self.insert_inode(&inode)?; // Insert the exact duplicate row into the active layer

        // 3. Copy all of its data blocks up to the active layer too!
        self.conn.execute(
            "INSERT INTO blocks (inode_id, layer_id, block_index, data)
             SELECT inode_id, 'active', block_index, data
             FROM blocks WHERE inode_id = ?1 AND layer_id = 'base'",
            params![inode_id],
        )?;

        Ok(inode)
    }
}
