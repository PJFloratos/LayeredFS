# LayeredFS

**LayeredFS** is a custom, version-controlled filesystem written in Rust. By bridging the Linux FUSE (Filesystem in Userspace) API with a relational SQLite database, it provides Git-like tracking, Copy-on-Write (CoW) data duplication, and real-time point-in-time traversal—all entirely transparent to standard POSIX applications.

---

## Key Features

* **Relational Storage Engine:** Files, directories, and binary blocks are stored relationally in SQLite, allowing for powerful queries and direct state inspection.
* **Copy-on-Write (CoW):** Modifying historical files automatically clones the required data blocks, preserving the immutable past while allowing seamless active edits.
* **Git-Like Version Control:**
* `commit`: Freeze the active layer into an immutable snapshot.
* `revert`: Discard all uncommitted changes, returning to a clean state.
* **Time Travel (`checkout`):** Dynamically filter the filesystem's union-view to mount historical commits in real-time.
* **Interactive Control Plane:** A thread-safe, concurrent CLI runs alongside the mounted filesystem to allow manual snapshotting and raw SQL debugging without unmounting.

---

## Project Structure

```text
src/
├── main.rs            # Orchestrator: Wires dependencies and mounts FUSE
├── control_plane.rs   # Background thread for the interactive CLI prompt
├── fuse/
│   ├── mod.rs         
│   └── engine.rs      # Implements the fuser::Filesystem POSIX traits
└── database/
    ├── mod.rs         # Database engine and state management
    ├── models.rs      # Structs mapping to SQLite tables (Layer, Inode, Block)
    ├── setup.rs       # Schema initialization and factory resetting
    ├── reads.rs       # Dynamic union-view queries and time-travel filters
    └── writes.rs      # Copy-on-Write mutations and Version Control logic
```

---

## Architecture

LayeredFS separates concerns into three distinct layers:

1. **The POSIX Layer (`fuser`):** Receives standard OS filesystem calls (`ls`, `cat`, `mkdir`, `echo`) and translates them into database operations.
2. **The Union Engine:** Dynamically stacks database rows based on a priority queue. It always serves the "newest" version of a file unless constrained by a time-travel checkout.
3. **The Control Plane:** A background CLI running in the `main` process that holds a shared `Arc<Mutex<LayeredDb>>` pointer, allowing you to manipulate database state and trigger version control operations on the fly.

---

## Installation & Setup

### Prerequisites

* **Rust:** Standard `cargo` toolchain.
* **Linux:** FUSE requires a Linux environment.
* **Dependencies:** `libfuse-dev` (or equivalent) must be installed on your host system.

### Running the Engine

Simply clone the repository and use `cargo run`. The engine will automatically initialize a physical `layeredfs.db` file, bootstrap the schema, and mount the filesystem to `/tmp/layeredfs_mount`.

---

## The Control Plane Interface

While the FUSE drive is mounted, your terminal will provide a `LayeredFS>` interactive prompt.

| Command | Description |
| --- | --- |
| `commit <name>` | Freezes all active changes into an immutable historical layer named `<name>`. |
| `checkout <name>` | Time-travels the OS mount to a specific commit, hiding any data that occurred after it. Use `checkout latest` to return to the present. |
| `revert` | Discards all uncommitted changes in the active layer. |
| `reset` | Drops all tables, wipes the database, and returns the filesystem to a clean factory state. |
| `sql <query>` | Executes raw SQL against the database engine and formats the output into a table. |

---

## Quick Start Tutorial

Open two terminal windows: one for the **Control Plane** (`cargo run`) and one for the **OS Mount** (`/tmp/layeredfs_mount`).

**1. Create some data (OS Terminal)**

```bash
echo "Hello from Layer 1" > /tmp/layeredfs_mount/file.txt

```

**2. Snapshot the state (Control Plane)**

```text
LayeredFS> commit v1

```

**3. Modify the data (OS Terminal)**

```bash
echo "Hello from Layer 2" >> /tmp/layeredfs_mount/file.txt

```

*(If you `cat` the file now, you will see both lines).*

**4. Time Travel to the past (Control Plane)**

```text
LayeredFS> checkout v1

```

**5. Observe the past (OS Terminal)**

```bash
cat /tmp/layeredfs_mount/file.txt

```

*(The file instantly reverts to only showing the first line, as if Layer 2 never happened!)*
