use crate::database::models::Inode;
use crate::database::LayeredDb;
use chrono::Utc;
use fuser::{FileType, Filesystem, ReplyAttr, ReplyData, ReplyDirectory, ReplyEntry, Request};
use libc::ENOENT;
use std::ffi::OsStr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

pub struct LayeredFsEngine {
    pub db: Arc<Mutex<LayeredDb>>,
}

const TTL: Duration = Duration::from_secs(1);

impl Filesystem for LayeredFsEngine {
    // ==========================================
    // READ OPERATIONS
    // ==========================================

    fn getattr(&mut self, _req: &Request, ino: u64, reply: ReplyAttr) {
        let db_lock = self.db.lock().unwrap();

        match db_lock.get_inode(ino) {
            Ok(inode) => reply.attr(&TTL, &inode.as_fuse_attr()),
            Err(_) => reply.error(ENOENT),
        }
    }

    fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
        let db_lock = self.db.lock().unwrap();
        let name_str = name.to_str().unwrap_or_default();

        match db_lock.get_inode_by_name(parent, name_str) {
            Ok(inode) => reply.entry(&TTL, &inode.as_fuse_attr(), 1),
            Err(_) => reply.error(ENOENT),
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
        let db_lock = self.db.lock().unwrap();

        let parent_ino = match db_lock.get_inode(ino) {
            Ok(inode) => inode.parent_id,
            Err(_) => {
                reply.error(ENOENT);
                return;
            }
        };

        let mut entries = vec![
            (ino, FileType::Directory, ".".to_string()),
            (parent_ino, FileType::Directory, "..".to_string()),
        ];

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

        for (i, entry) in entries.into_iter().enumerate().skip(offset as usize) {
            if reply.add(entry.0, (i + 1) as i64, entry.1, entry.2) {
                break;
            }
        }
        reply.ok();
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

        match db_lock.get_block(ino, 0) {
            // Simplified to block 0 for now
            Ok(block) => {
                let start = offset as usize;
                if start >= block.data.len() {
                    reply.data(&[]);
                    return;
                }
                let end = std::cmp::min(block.data.len(), start + (size as usize));
                reply.data(&block.data[start..end]);
            }
            Err(_) => reply.error(ENOENT),
        }
    }

    // ==========================================
    // WRITE OPERATIONS
    // ==========================================

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
        let name_str = name.to_str().unwrap_or_default();

        let new_dir = Inode {
            inode_id: db_lock.get_next_inode_id().unwrap_or(9999), // Simplified error handling
            layer_id: "active".to_string(),
            parent_id: parent,
            name: name_str.to_string(),
            file_type: "dir".to_string(),
            size: 4096,
            permissions: mode as u16,
            mtime: Utc::now().to_rfc3339(),
            semantic_summary: None,
        };

        if db_lock.insert_inode(&new_dir).is_ok() {
            reply.entry(&TTL, &new_dir.as_fuse_attr(), 1);
        } else {
            reply.error(libc::EIO);
        }
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
        let name_str = name.to_str().unwrap_or_default();

        let new_file = Inode {
            inode_id: db_lock.get_next_inode_id().unwrap_or(9999),
            layer_id: "active".to_string(),
            parent_id: parent,
            name: name_str.to_string(),
            file_type: "file".to_string(),
            size: 0,
            permissions: mode as u16,
            mtime: Utc::now().to_rfc3339(),
            semantic_summary: None,
        };

        if db_lock.insert_inode(&new_file).is_ok() {
            reply.entry(&TTL, &new_file.as_fuse_attr(), 1);
        } else {
            reply.error(libc::EIO);
        }
    }

    fn unlink(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: fuser::ReplyEmpty) {
        let db_lock = self.db.lock().unwrap();
        let name_str = name.to_str().unwrap_or_default();

        // 1. Find the existing file's ID
        let inode = match db_lock.get_inode_by_name(parent, name_str) {
            Ok(i) => i,
            Err(_) => return reply.error(ENOENT),
        };

        // 2. Overwrite it with a tombstone using the EXACT SAME inode_id
        let tombstone = Inode {
            inode_id: inode.inode_id,
            layer_id: "active".to_string(),
            parent_id: parent,
            name: name_str.to_string(),
            file_type: "tombstone".to_string(),
            size: 0,
            permissions: 0,
            mtime: Utc::now().to_rfc3339(),
            semantic_summary: Some("Deleted".to_string()),
        };

        if db_lock.insert_inode(&tombstone).is_ok() {
            // 3. (Optional but good) Delete the binary data from the active layer to save space
            let _ = db_lock.conn.execute(
                "DELETE FROM blocks WHERE inode_id = ?1 AND layer_id = 'active'",
                rusqlite::params![inode.inode_id],
            );
            reply.ok();
        } else {
            reply.error(libc::EIO);
        }
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

        let mut inode = match db_lock.copy_on_write(ino) {
            Ok(i) => i,
            Err(_) => return reply.error(ENOENT),
        };

        if let Some(m) = mode {
            inode.permissions = m as u16;
        }
        if let Some(s) = size {
            inode.size = s;
        }
        if mtime.is_some() {
            inode.mtime = Utc::now().to_rfc3339();
        }

        if db_lock.update_inode(&inode).is_ok() {
            reply.attr(&TTL, &inode.as_fuse_attr());
        } else {
            reply.error(libc::EIO);
        }
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

        let mut inode = match db_lock.copy_on_write(ino) {
            Ok(i) => i,
            Err(_) => return reply.error(ENOENT),
        };

        let mut block = db_lock
            .get_block(ino, 0)
            .unwrap_or(crate::database::models::Block {
                inode_id: ino,
                layer_id: "active".to_string(),
                block_index: 0,
                data: Vec::new(),
            });

        let start = offset as usize;
        let end = start + data.len();

        if block.data.len() < end {
            block.data.resize(end, 0);
        }
        block.data[start..end].copy_from_slice(data);

        if db_lock.upsert_block(&block).is_err() {
            return reply.error(libc::EIO);
        }

        if (end as u64) > inode.size {
            inode.size = end as u64;
            inode.mtime = Utc::now().to_rfc3339();
            if db_lock.update_inode(&inode).is_err() {
                return reply.error(libc::EIO);
            }
        }

        reply.written(data.len() as u32);
    }
}
