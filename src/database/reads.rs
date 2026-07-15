use crate::database::models::{Block, Inode};
use crate::database::LayeredDb;
use rusqlite::{params, Result};

impl LayeredDb {
    // Helper to get the current limit of our time machine
    fn get_max_priority(&self) -> i32 {
        self.head_priority.unwrap_or(i32::MAX)
    }

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
            "SELECT b.inode_id, b.layer_id, b.block_index, b.data
             FROM blocks b
             JOIN layers l ON b.layer_id = l.layer_id
             WHERE b.inode_id = ?1 AND b.block_index = ?2 AND l.priority <= ?3
             ORDER BY l.priority DESC
             LIMIT 1",
            params![inode_id, block_index, self.get_max_priority()],
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

    pub fn execute_raw_sql(&self, query: &str) -> Result<Vec<String>> {
        // 1. Prepare the statement dynamically
        let mut stmt = self.conn.prepare(query)?;
        let column_count = stmt.column_count();

        // 2. If it's a mutation query (INSERT, UPDATE, DELETE) with no returning columns
        if column_count == 0 {
            let changes = self.conn.execute(query, [])?;
            return Ok(vec![format!("SUCCESS: {} rows affected.", changes)]);
        }

        // 3. If it's a SELECT query, dynamically fetch column names
        let column_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
        let header = column_names.join(" | ");

        let mut results = vec![header.clone()];
        results.push("-".repeat(header.len())); // Create a visual separator line

        // 4. Iterate over the rows and dynamically parse the data types
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let mut row_strings = Vec::new();
            for i in 0..column_count {
                let val_ref = row.get_ref(i)?;

                // Match the SQLite type to a printable Rust String
                let val_str = match val_ref {
                    rusqlite::types::ValueRef::Null => "NULL".to_string(),
                    rusqlite::types::ValueRef::Integer(i) => i.to_string(),
                    rusqlite::types::ValueRef::Real(f) => f.to_string(),
                    rusqlite::types::ValueRef::Text(t) => String::from_utf8_lossy(t).into_owned(),
                    rusqlite::types::ValueRef::Blob(b) => format!("<BLOB {} bytes>", b.len()),
                };
                row_strings.push(val_str);
            }
            results.push(row_strings.join(" | "));
        }

        Ok(results)
    }
}
