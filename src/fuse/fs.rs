use crate::database::LayeredDb;
use fuser::{
    FileAttr, FileType, Filesystem, ReplyAttr, ReplyData, ReplyDirectory, ReplyEntry, Request,
};
use libc::ENOENT;
use std::ffi::OsStr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

/// This is the bridge between the Kernel and our SQLite database.
pub struct LayeredFsEngine {
    pub db: Arc<Mutex<LayeredDb>>,
}

// We define a Time-To-Live for the kernel's internal cache.
// 1 second means the kernel won't ask us for this inode again for 1 second.
const TTL: Duration = Duration::from_secs(1);

impl Filesystem for LayeredFsEngine {
    fn getattr(&mut self, _req: &Request, ino: u64, reply: ReplyAttr) {
        // Lock the database for this thread.
        let db_lock = self.db.lock().unwrap(); // Rust automatically unlocks it when the function ends

        // 1. Ask SQLite for the Inode
        match db_lock.get_inode(ino) {
            // 2. If it exists in the DB, translate it to a FUSE FileAttr
            Ok(inode) => {
                let kind = if inode.file_type == "dir" {
                    FileType::Directory
                } else {
                    FileType::RegularFile
                };

                let attr = FileAttr {
                    ino: inode.inode_id,
                    size: inode.size,
                    blocks: (inode.size + 511) / 512, // Calculate required 512-byte blocks
                    atime: SystemTime::now(),         // For now, we mock the timestamps
                    mtime: SystemTime::now(),
                    ctime: SystemTime::now(),
                    crtime: SystemTime::now(),
                    kind,
                    perm: inode.permissions,
                    nlink: if kind == FileType::Directory { 2 } else { 1 },
                    uid: 1000, // Hardcoded to standard primary user for now
                    gid: 1000,
                    rdev: 0,
                    blksize: 4096,
                    flags: 0,
                };

                // Send the successful attribute back to the kernel
                reply.attr(&TTL, &attr);
            }

            // 3. If SQLite throws an error (e.g., row not found), tell the kernel "No Entity"
            Err(_) => {
                reply.error(ENOENT);
            }
        }
    }

    fn readdir(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        // Lock the database for this thread.
        let db_lock = self.db.lock().unwrap();

        // 1. Fetch the current directory to find its parent ID (for the ".." entry)
        let parent_ino = match db_lock.get_inode(ino) {
            Ok(inode) => inode.parent_id,
            Err(_) => {
                reply.error(ENOENT);
                return;
            }
        };

        // 2. Build our list of entries: (Inode ID, File Type, Name)
        let mut entries = vec![
            (ino, FileType::Directory, ".".to_string()),
            (parent_ino, FileType::Directory, "..".to_string()),
        ];

        // 3. Query SQLite for actual files inside this folder
        if let Ok(children) = db_lock.get_directory_children(ino) {
            for child in children {
                let kind = if child.file_type == "dir" {
                    FileType::Directory
                } else {
                    FileType::RegularFile
                };
                entries.push((child.inode_id, kind, child.name));
            }
        }

        // 4. Send entries to the kernel, respecting the offset (pagination)
        for (i, entry) in entries.into_iter().enumerate().skip(offset as usize) {
            // reply.add returns true if the kernel's buffer is full.
            // (i + 1) tells the kernel what offset to ask for next time.
            let buffer_full = reply.add(entry.0, (i + 1) as i64, entry.1, entry.2);

            if buffer_full {
                break;
            }
        }

        // 5. Tell the kernel we are done sending data
        reply.ok();
    }

    fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
        let db_lock = self.db.lock().unwrap();

        // Convert the OS string to a standard Rust string
        let name_str = match name.to_str() {
            Some(s) => s,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        // Query the DB
        match db_lock.get_inode_by_name(parent, name_str) {
            Ok(inode) => {
                let kind = if inode.file_type == "dir" {
                    FileType::Directory
                } else {
                    FileType::RegularFile
                };

                let attr = FileAttr {
                    ino: inode.inode_id,
                    size: inode.size,
                    blocks: (inode.size + 511) / 512,
                    atime: SystemTime::now(),
                    mtime: SystemTime::now(),
                    ctime: SystemTime::now(),
                    crtime: SystemTime::now(),
                    kind,
                    perm: inode.permissions,
                    nlink: if kind == FileType::Directory { 2 } else { 1 },
                    uid: 1000,
                    gid: 1000,
                    rdev: 0,
                    blksize: 4096,
                    flags: 0,
                };

                // Reply with the entry attributes. The `1` is a generation number
                // used for NFS network caching. We can leave it at 1.
                reply.entry(&TTL, &attr, 1);
            }
            Err(_) => {
                reply.error(ENOENT);
            }
        }
    }

    fn read(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        size: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: ReplyData,
    ) {
        let db_lock = self.db.lock().unwrap();

        // In a production system, we would use the `offset` and `size` to
        // figure out exactly which 4KB block(s) to fetch.
        // For our dummy file, we know all the text fits in block 0.
        let block_index = 0;

        match db_lock.get_block(ino, block_index) {
            Ok(block) => {
                // If the kernel asks for an offset beyond our data, return empty bytes
                if offset as usize >= block.data.len() {
                    reply.data(&[]);
                    return;
                }

                // Slice the data based on what the kernel requested
                let end = std::cmp::min(block.data.len(), (offset as usize) + (size as usize));
                let requested_data = &block.data[(offset as usize)..end];

                // Send the bytes to the terminal!
                reply.data(requested_data);
            }
            Err(_) => {
                reply.error(ENOENT);
            }
        }
    }
}
