use fuser::{FileAttr, FileType};
use std::time::SystemTime;

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

impl Inode {
    /// Converts our Database Inode into a POSIX-compliant FUSE FileAttr
    pub fn as_fuse_attr(&self) -> FileAttr {
        let kind = if self.file_type == "dir" {
            FileType::Directory
        } else {
            FileType::RegularFile
        };

        FileAttr {
            ino: self.inode_id,
            size: self.size,
            blocks: (self.size + 511) / 512,
            atime: SystemTime::now(), // In a real system, we'd parse self.mtime here
            mtime: SystemTime::now(),
            ctime: SystemTime::now(),
            crtime: SystemTime::now(),
            kind,
            perm: self.permissions,
            nlink: if kind == FileType::Directory { 2 } else { 1 },
            uid: 1000,
            gid: 1000,
            rdev: 0,
            blksize: 4096,
            flags: 0,
        }
    }
}
