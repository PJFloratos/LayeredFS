# Project: LayeredFS

## Design Specification & Architecture Document

### 1. Project Overview & Vision

**LayeredFS** is a high-performance, user-space, versioned filesystem implemented in **Rust** using the **FUSE (Filesystem in Userspace)** protocol. LayeredFS bridges the gap between high-level database abstraction and low-level kernel I/O operations by delegating all data persistence, metadata management, and hierarchical state to a **Relational DBMS (SQLite)** backend.

Unlike traditional filesystems, LayeredFS implements a **Multi-Layered Stack** (similar to OverlayFS or container images), completely decoupling data persistence from a static directory structure.

* **Core Vision:** A file-system that treats OS state as queryable, versioned data rather than a static directory of unstructured bits. It acts as an "OS API" for autonomous agents, providing programmatic control over system state.
* **Key Philosophy:** *"Data is an Inode, State is a Row, Time is a Layer."*

---

### 2. System Architecture

The system functions as a highly-concurrent request-processing loop. The Linux kernel sends system calls to the FUSE driver, which acts as the application layer, resolving POSIX paths into deterministic SQL transactions.

#### 2.1 The Layered Storage Engine

LayeredFS abstracts file operations into relational transactions operating across stacked virtual environments.

* **Base Layer (Immutable):** A read-only snapshot representing a "Clean System State" or a specific checkpoint in time.
* **Active Layer (Writable):** A dedicated, volatile or persistent namespace capturing current modifications (Copy-on-Write).
* **Merged View (Union Engine):** The FUSE driver computes the union of layers in real-time. If an inode or data block exists in the Active Layer, it "shadows" (overrides) the version in the Base Layer.

#### 2.2 Relational Schema (The Data Plane)

To handle files larger than RAM and maintain hierarchical integrity, the database relies on three core tables:

1. **`layers` Table:** Defines the stack topology.
   * *Columns:* `layer_id`, `parent_layer_id`, `priority`, `is_readonly`, `created_at`.
2. **`inodes` Table:** Stores metadata and acts as the hierarchical tree structure.
   * *Columns:* `inode_id`, `layer_id`, `parent_id`, `name`, `type` (file/dir/symlink), `size`, `permissions`, `mtime`, `semantic_summary` (for Agentic searches).
3. **`blocks` Table:** Stores raw binary data fragmented into fixed-size (e.g., 4KB) `BLOB` chunks. Enables Content-Addressable Storage (CAS) logic.
   * *Columns:* `inode_id`, `layer_id`, `block_index`, `data`.

---

### 3. Core Mechanisms & Engineering Challenges

Addressing systems-level hurdles requires a blend of database optimization and low-level kernel trickery.

#### 3.1 I/O Latency Mitigation & Caching
* **Challenge:** A raw recursive SQL query for every byte read or path resolution is too slow for standard OS operations.
* **Solution:** Implement a multi-level **LRU Cache** (`lru` crate) in user-space.
  * *Path Cache:* Maps `/docs/photo.jpg` directly to its resolved `inode_id`.
  * *Block Cache:* Holds frequently accessed 4KB data chunks in RAM to eliminate redundant database hits.

#### 3.2 Atomicity, Durability & POSIX Compliance
* **Challenge:** Surviving crashes while maintaining the specific error codes the kernel expects (e.g., `ENOENT`, `EACCES`).
* **Solution:** Utilize SQLite’s **ACID-compliant transactions**. Every `flush` or `fsync` operation from the kernel triggers a transaction commit. All SQLite error states are mapped via a rigid translation layer to POSIX `errno` codes.

#### 3.3 The Union Engine & Copy-on-Write (CoW)
* **Challenge:** Modifying a file that exists in the Base Layer without altering the immutable record.
* **Solution:** When a write request targets a Base Layer inode, LayeredFS triggers a **CoW event**: it duplicates the inode metadata and required blocks to the Active Layer, linking them via `layer_id`, and performs the write on the new shadowed copy.

---

### 4. Agentic & Management API

LayeredFS goes beyond standard human user interfaces by exposing an "Agent API," making it ideal for LLM-driven automation.

* **Natural Language Control via SQL:** Agents can perform complex filesystem management deterministically. (e.g., executing `DELETE FROM inodes WHERE type='tmp' AND mtime < datetime('now', '-2 hours')`).
* **Simulation & Dry-Runs (`check_impact`):** Agents can simulate the effect of a delete/move operation and query the transaction rollback state before finalizing.
* **Self-Summarization:** A dedicated `semantic_summary` metadata field allows agents to search files by contextual meaning rather than standard path traversal.

---

### 5. Technical Stack

* **Language:** Rust (`edition 2024` or latest) - ensuring safety-first systems programming.
* **FUSE Bridge:** `fuser` (low-level crate for kernel communication).
* **Persistence:** `rusqlite` (ACID-compliant state management).
* **Concurrency:** `std::sync::{Arc, RwLock}` for thread-safe, cross-layer database access.
* **Agentic Interface:** `serde` + `tokio` (for JSON-RPC based communication with external LLMs).
* **Logging & Tracing:** `env_logger` / `tracing` (capturing all FUSE requests to debug kernel-space states).

---

### 6. Implementation Roadmap

| Phase | Focus | Core Deliverable |
| :--- | :--- | :--- |
| **Phase 1** | **Foundation** | "Hello World" FUSE mount point in Rust, static file read. |
| **Phase 2** | **Metadata Schema** | SQLite integration; Implementing `getattr`, `readdir`, and `lookup`. |
| **Phase 3** | **Data Block & Union Engine** | Implementing `read`/`write` with 4KB chunking; "Shadowing" logic merging Base and Active layers. |
| **Phase 4** | **Caching & Version Control** | LRU caching for performance; `commit` and `rollback` logic for layers (Container-like snapshotting). |
| **Phase 5** | **Agent API & Compliance** | JSON-RPC interface for LLMs; Full POSIX compliance (`mkdir`, `unlink`, `rename`, permissions). |

---

### 7. Success Metrics

* **Correctness:** Surviving standard file-operation stress tests (like `fsstress`) and successfully running standard CLI tools (`cp`, `ls`, `vim`, `git`) inside the mount point.
* **Performance:** Achieving acceptable I/O latency for standard operations, competitive with user-space equivalents (like `sshfs`), relying heavily on LRU hit rates.
* **Agentic Utility:** Successfully allowing an automated script or LLM to branch a filesystem state, perform isolated operations, and query the differential state via SQL.
