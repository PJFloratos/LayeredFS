mod database;
mod fuse;

use database::LayeredDb;
use fuse::engine::LayeredFsEngine;
use rusqlite::Result;
use std::fs;
use std::sync::{Arc, Mutex};

fn main() -> Result<()> {
    env_logger::init();
    println!("Starting LayeredFS...");

    // 1. Boot up the database
    let db = LayeredDb::new()?;

    // 2. Delegate the messy setup to the module
    db.bootstrap_initial_state()?;
    println!("SQLite Schema and Root Inode initialized successfully.");

    // 3. Set up the FUSE mount point
    let mountpoint = "/tmp/layeredfs_mount";
    if let Err(e) = fs::create_dir_all(mountpoint) {
        println!("Failed to create mountpoint: {}", e);
        return Ok(());
    }

    println!("Mounting LayeredFS to {}...", mountpoint);

    // 4. Wrap the DB in Arc<Mutex> for thread-safe sharing
    let shared_db = Arc::new(Mutex::new(db));

    // 5. Create our FUSE engine, handing it a CLONE of the Arc pointer.
    // This allows main.rs, FUSE, and future modules to share the DB safely.
    let fs_engine = LayeredFsEngine {
        db: Arc::clone(&shared_db),
    };

    // 6. Mount the filesystem
    let options = vec![fuser::MountOption::FSName("layeredfs".to_string())];

    match fuser::mount2(fs_engine, mountpoint, &options) {
        Ok(_) => println!("Filesystem unmounted successfully."),
        Err(e) => println!("FUSE error: {}", e),
    }

    Ok(())
}
