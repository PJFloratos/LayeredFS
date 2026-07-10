use crate::database::models::Inode;
use crate::database::LayeredDb;
use chrono::Utc;
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

    fn mkdir(
        &mut self,
        _req: &Request,
        parent: u64,
        name: &OsStr,
        mode: u32,
        _umask: u32,
        reply: ReplyEntry,
    ) {
        let db_lock = self.db.lock().unwrap();

        let name_str = match name.to_str() {
            Some(s) => s,
            None => {
                reply.error(libc::EINVAL); // Invalid argument if name isn't UTF-8
                return;
            }
        };

        // 1. Get the next available ID
        let new_inode_id = match db_lock.get_next_inode_id() {
            Ok(id) => id,
            Err(_) => {
                reply.error(libc::EIO); // I/O Error
                return;
            }
        };

        let current_time = Utc::now().to_rfc3339();

        // 2. Construct the new directory Inode assigned strictly to the ACTIVE layer
        let new_dir = Inode {
            inode_id: new_inode_id,
            layer_id: "active".to_string(), // <- The critical routing step
            parent_id: parent,
            name: name_str.to_string(),
            file_type: "dir".to_string(),
            size: 4096,
            permissions: mode as u16,
            mtime: current_time,
            semantic_summary: None,
        };

        // 3. Insert it into SQLite
        if db_lock.insert_inode(&new_dir).is_err() {
            reply.error(libc::EIO);
            return;
        }

        // 4. Construct the POSIX attributes to send back to the kernel
        let attr = FileAttr {
            ino: new_dir.inode_id,
            size: new_dir.size,
            blocks: (new_dir.size + 511) / 512,
            atime: SystemTime::now(),
            mtime: SystemTime::now(),
            ctime: SystemTime::now(),
            crtime: SystemTime::now(),
            kind: FileType::Directory,
            perm: new_dir.permissions,
            nlink: 2,
            uid: 1000,
            gid: 1000,
            rdev: 0,
            blksize: 4096,
            flags: 0,
        };

        // 5. Reply with success
        reply.entry(&TTL, &attr, 1);
    }

    fn mknod(
        &mut self,
        _req: &Request,
        parent: u64,
        name: &OsStr,
        mode: u32,
        _umask: u32,
        _rdev: u32,
        reply: ReplyEntry,
    ) {
        let db_lock = self.db.lock().unwrap();

        let name_str = match name.to_str() {
            Some(s) => s,
            None => {
                reply.error(libc::EINVAL);
                return;
            }
        };

        let new_inode_id = match db_lock.get_next_inode_id() {
            Ok(id) => id,
            Err(_) => {
                reply.error(libc::EIO);
                return;
            }
        };

        let current_time = Utc::now().to_rfc3339();

        let new_file = Inode {
            inode_id: new_inode_id,
            layer_id: "active".to_string(), // Write strictly to Active Layer
            parent_id: parent,
            name: name_str.to_string(),
            file_type: "file".to_string(),
            size: 0,
            permissions: mode as u16,
            mtime: current_time,
            semantic_summary: None,
        };

        if db_lock.insert_inode(&new_file).is_err() {
            reply.error(libc::EIO);
            return;
        }

        let attr = FileAttr {
            ino: new_file.inode_id,
            size: new_file.size,
            blocks: 0,
            atime: SystemTime::now(),
            mtime: SystemTime::now(),
            ctime: SystemTime::now(),
            crtime: SystemTime::now(),
            kind: FileType::RegularFile,
            perm: new_file.permissions,
            nlink: 1,
            uid: 1000,
            gid: 1000,
            rdev: 0,
            blksize: 4096,
            flags: 0,
        };

        reply.entry(&TTL, &attr, 1);
    }

    fn unlink(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: fuser::ReplyEmpty) {
        let db_lock = self.db.lock().unwrap();

        let name_str = match name.to_str() {
            Some(s) => s,
            None => {
                reply.error(libc::EINVAL);
                return;
            }
        };

        let new_inode_id = match db_lock.get_next_inode_id() {
            Ok(id) => id,
            Err(_) => {
                reply.error(libc::EIO);
                return;
            }
        };

        // We do NOT delete the row. We insert a TOMBSTONE in the active layer.
        let tombstone = Inode {
            inode_id: new_inode_id,
            layer_id: "active".to_string(),
            parent_id: parent,
            name: name_str.to_string(),
            file_type: "tombstone".to_string(), // The magic marker
            size: 0,
            permissions: 0,
            mtime: Utc::now().to_rfc3339(),
            semantic_summary: Some("Deleted".to_string()),
        };

        if db_lock.insert_inode(&tombstone).is_err() {
            reply.error(libc::EIO);
            return;
        }

        reply.ok();
    }

    fn setattr(
        &mut self,
        _req: &Request,
        ino: u64,
        mode: Option<u32>,
        _uid: Option<u32>,
        _gid: Option<u32>,
        size: Option<u64>,
        _atime: Option<fuser::TimeOrNow>,
        mtime: Option<fuser::TimeOrNow>,
        _ctime: Option<SystemTime>,
        _fh: Option<u64>,
        _crtime: Option<SystemTime>,
        _chgtime: Option<SystemTime>,
        _bkuptime: Option<SystemTime>,
        _flags: Option<u32>,
        reply: ReplyAttr,
    ) {
        let db_lock = self.db.lock().unwrap();

        // 1. Pass the file through our Copy-on-Write engine
        let mut inode = match db_lock.copy_on_write(ino) {
            Ok(i) => i,
            Err(_) => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        // 2. Apply requested changes from the OS
        if let Some(m) = mode {
            inode.permissions = m as u16;
        }
        if let Some(s) = size {
            inode.size = s;
        }
        if mtime.is_some() {
            inode.mtime = chrono::Utc::now().to_rfc3339();
        }

        // 3. Save the modified active inode to the database
        if db_lock.update_inode(&inode).is_err() {
            reply.error(libc::EIO);
            return;
        }

        // 4. Tell the kernel it succeeded
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

        reply.attr(&TTL, &attr);
    }

    fn write(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        data: &[u8],
        _write_flags: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: fuser::ReplyWrite,
    ) {
        let db_lock = self.db.lock().unwrap();

        // 1. Pass the file through our Copy-on-Write engine
        let mut inode = match db_lock.copy_on_write(ino) {
            Ok(i) => i,
            Err(_) => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        // 2. Fetch the block (or create an empty one if it's a new file)
        let block_index = 0;
        let mut block = match db_lock.get_block(ino, block_index) {
            Ok(b) => b,
            Err(_) => {
                crate::database::models::Block {
                    inode_id: ino,
                    layer_id: "active".to_string(), // Strictly tied to the active layer!
                    block_index,
                    data: Vec::new(),
                }
            }
        };

        // 3. Splice the new bytes into the block's data vector
        let start = offset as usize;
        let end = start + data.len();

        // If the OS tells us to write past the end of our current data, pad it with zeros
        if block.data.len() < end {
            block.data.resize(end, 0);
        }

        // Overwrite the specific slice of data with the incoming bytes
        block.data[start..end].copy_from_slice(data);

        // 4. Save the modified block back to SQLite
        if db_lock.upsert_block(&block).is_err() {
            reply.error(libc::EIO);
            return;
        }

        // 5. Update the Inode size if we made the file larger
        if (end as u64) > inode.size {
            inode.size = end as u64;
            inode.mtime = chrono::Utc::now().to_rfc3339();

            if db_lock.update_inode(&inode).is_err() {
                reply.error(libc::EIO);
                return;
            }
        }

        // 6. Tell the kernel exactly how many bytes we successfully wrote
        reply.written(data.len() as u32);
    }
}
