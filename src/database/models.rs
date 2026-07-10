#[derive(Debug, Clone)]
pub struct Layer {
    pub layer_id: String,
    pub parent_layer_id: Option<String>,
    pub priority: i32,
    pub is_readonly: bool,
    pub created_at: String, // We use String for SQLite DATETIME right now
}

#[derive(Debug, Clone)]
pub struct Inode {
    pub inode_id: u64,
    pub layer_id: String,
    pub parent_id: u64,
    pub name: String,
    pub file_type: String, // "file", "dir", or "symlink"
    pub size: u64,
    pub permissions: u16,
    pub mtime: String,
    pub semantic_summary: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Block {
    pub inode_id: u64,
    pub layer_id: String,
    pub block_index: i64,
    pub data: Vec<u8>, // Raw binary data
}
