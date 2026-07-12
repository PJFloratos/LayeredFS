use crate::database::models::{Block, Inode, Layer};
use crate::database::LayeredDb;
use rusqlite::{params, Result};

impl LayeredDb {
    pub fn insert_layer(&self, layer: &Layer) -> Result<()> {
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

    pub fn insert_inode(&self, inode: &Inode) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO inodes (inode_id, layer_id, parent_id, name, file_type, size, permissions, mtime, semantic_summary)
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

    pub fn copy_on_write(&self, inode_id: u64) -> Result<Inode> {
        let mut inode = self.get_inode(inode_id)?;

        if inode.layer_id == "active" {
            return Ok(inode);
        }

        inode.layer_id = "active".to_string();
        inode.mtime = chrono::Utc::now().to_rfc3339();
        self.insert_inode(&inode)?;

        self.conn.execute(
            "INSERT INTO blocks (inode_id, layer_id, block_index, data)
             SELECT inode_id, 'active', block_index, data
             FROM blocks WHERE inode_id = ?1 AND layer_id = 'base'",
            params![inode_id],
        )?;

        Ok(inode)
    }

    pub fn commit(&self, commit_name: &str) -> Result<()> {
        // 1. Get the current active layer's priority[cite: 14]
        let current_priority: i32 = self.conn.query_row(
            "SELECT priority FROM layers WHERE layer_id = 'active'",
            [],
            |row| row.get(0),
        )?;

        // 2. Rename 'active' to the new commit name across all tables[cite: 14]
        self.conn.execute(
            "UPDATE layers SET layer_id = ?1, is_readonly = true WHERE layer_id = 'active'",
            params![commit_name],
        )?;
        self.conn.execute(
            "UPDATE inodes SET layer_id = ?1 WHERE layer_id = 'active'",
            params![commit_name],
        )?;
        self.conn.execute(
            "UPDATE blocks SET layer_id = ?1 WHERE layer_id = 'active'",
            params![commit_name],
        )?;

        // 3. Create a brand new empty 'active' layer sitting on top of the commit[cite: 14]
        let new_active = Layer {
            layer_id: "active".to_string(),
            parent_layer_id: Some(commit_name.to_string()),
            priority: current_priority + 1, // Must be higher so it shadows the commit!
            is_readonly: false,
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        self.insert_layer(&new_active)?;

        Ok(())
    }
}
