use crate::database::models::{Block, Inode};
use crate::database::LayeredDb;
use rusqlite::{params, Result};

impl LayeredDb {
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

        if inode.file_type == "tombstone" {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        Ok(inode)
    }

    pub fn get_inode_by_name(&self, parent_id: u64, name: &str) -> Result<Inode> {
        let inode = self.conn.query_row(
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

        if inode.file_type == "tombstone" {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        Ok(inode)
    }

    pub fn get_next_inode_id(&self) -> Result<u64> {
        self.conn.query_row(
            "SELECT COALESCE(MAX(inode_id), 0) + 1 FROM inodes",
            [],
            |row| row.get(0),
        )
    }

    pub fn get_directory_children(&self, parent_id: u64) -> Result<Vec<Inode>> {
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
                    data: row.get(3)?,
                })
            },
        )
    }
}
