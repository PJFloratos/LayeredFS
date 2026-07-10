# LayeredFS Architecture Document

## Overview
LayeredFS is a custom POSIX-compliant virtual filesystem implemented in Userspace (FUSE) using Rust. Unlike traditional filesystems that write raw blocks to a disk partition, LayeredFS uses a **Relational Database (SQLite)** as its storage backend.

Its defining feature is a **Copy-on-Write (CoW) Layering Engine**, similar in concept to Docker's OverlayFS. It allows for an immutable "Base" state and a writable "Active" state, enabling non-destructive edits, time-travel/versioning capabilities, and robust transactional file operations.

---

## Core Concepts

### 1. The Union View
When the OS requests the contents of a directory, LayeredFS does not just return a flat list of files. It calculates a "Union View" on the fly. By using SQLite window functions (`ROW_NUMBER() OVER (PARTITION BY name ORDER BY priority DESC)`), the filesystem dynamically stacks layers on top of each other. If `hello.txt` exists in both the Base and Active layers, the engine only presents the version from the highest-priority layer to the OS.

### 2. Copy-on-Write (CoW)
The Base layer is strictly read-only. When a user modifies an existing file from the Base layer (via `write` or `setattr`), the CoW engine intervenes:
1. It intercepts the write request.
2. It clones the metadata (`Inode`) and raw binary data (`Block`) of the file into the Active layer.
3. It applies the requested modifications exclusively to the Active layer's copy.
The underlying Base layer remains permanently untouched and intact.

### 3. Tombstone Deletions
Because the Base layer is immutable, files originating from it cannot be physically deleted via a SQL `DELETE` command. Instead, LayeredFS uses **Tombstoning**.
When `unlink` is called, the system inserts a new metadata record into the Active layer with the exact same `inode_id` and name, but with a `file_type` of `"tombstone"`. The Union View detects this magic marker and actively hides the file from the OS, simulating a successful deletion while preserving the original data.

---

## Database Schema

The SQLite backend is normalized into three primary tables:

* **`layers`**: Manages the hierarchy of filesystem states.
  * `layer_id` (Primary Key)
  * `priority` (Higher integer = higher precedence in the Union View)
  * `is_readonly` (Defines if the CoW engine must be triggered)

* **`inodes`**: Stores all POSIX metadata (names, permissions, timestamps, sizes).
  * **Composite Primary Key:** `(inode_id, layer_id)`. This allows the exact same file (Inode 2) to exist simultaneously in both the Base and Active layers.
  * `parent_id` (For directory tree traversal)
  * `file_type` (`"dir"`, `"file"`, or `"tombstone"`)

* **`blocks`**: Stores the actual binary payload of files.
  * **Composite Primary Key:** `(inode_id, layer_id, block_index)`.
  * `data` (Stored as an SQLite `BLOB`, mapping to Rust's `Vec<u8>`).

---

## Codebase Modules & Responsibilities

The Rust codebase is strictly decoupled to separate data representation, persistence, and OS routing.

### `src/database/` (Storage & State)
This module acts as the ORM and domain logic handler. It takes complete ownership of all SQL execution.
* **`mod.rs`**: Gatekeeper and struct definition. Holds the `rusqlite::Connection`.
* **`setup.rs`**: Handles DDL (schema creation) and bootstraps the initial system state (seeding Base/Active layers and the Root Directory).
* **`models.rs`**: Defines Rust structs (`Layer`, `Inode`, `Block`). Implements the critical `as_fuse_attr()` method to keep FUSE logic DRY by teaching the Database Inode how to convert itself into a POSIX FileAttr.
* **`reads.rs`**: Executes read-only queries (`SELECT`). Implements the Union View logic for `lookup` and `readdir`.
* **`writes.rs`**: Executes state mutations (`INSERT`, `UPDATE`, `DELETE`). Houses the crucial `copy_on_write` engine logic.

### `src/fuse/` (The OS Bridge)
This module acts purely as a translator between Linux Kernel requests and the Database engine.
* **`engine.rs`**: Implements the `fuser::Filesystem` trait. It receives raw byte streams and inode requests from the OS, locks the shared database state, routes the request to the appropriate `database` module method, and packages the result back into FUSE `Reply` structures.

### `src/main.rs` (The Orchestrator)
The entry point of the application. It initializes logging, boots the database, triggers the bootstrap sequence, mounts the filesystem to `/tmp/layeredfs_mount`, and wraps the DB in an `Arc<Mutex>` to ensure thread-safe concurrency for the FUSE engine and future Agent APIs.

---

## Concurrency Model
The filesystem uses standard Rust synchronization primitives to handle highly concurrent OS requests.
The database is wrapped in an `Arc<Mutex<LayeredDb>>`. Every FUSE callback explicitly locks this mutex upon entry and drops the lock upon exit. This guarantees serialized access to the SQLite connection, preventing database locks/corruption while paving the way for multi-threaded background workers (e.g., AI Agents, sync engines).

## Current Status & Next Steps
* **Status:** The core data plane and metadata plane are fully operational. Creation, reading, updating, and deletion (CRUD) are POSIX compliant via the CoW engine.
* **Current Limitation:** Storage is currently configured as `Connection::open_in_memory()`, meaning all data is volatile and lost upon unmounting. Binary data storage is simplified to a single block (`block_index = 0`) containing the entire file payload.
