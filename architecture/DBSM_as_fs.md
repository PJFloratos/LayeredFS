### 1. The Mapping: From POSIX to SQL

To make a database act like a filesystem, you have to map the **POSIX API** (the functions the Kernel calls) to **SQL Queries**.

When the Kernel asks for a file, it uses these specific "handles." Here is how you map them to your database:

| POSIX Call | Purpose | SQL Equivalent |
| --- | --- | --- |
| `lookup` | Find a file by name in a directory | `SELECT * FROM inodes WHERE parent_id = ? AND name = ?` |
| `getattr` | Get size, permissions, MTime | `SELECT * FROM inodes WHERE inode_id = ?` |
| `readdir` | List files in a folder | `SELECT * FROM inodes WHERE parent_id = ?` |
| `read` | Read data at an offset | `SELECT data FROM blocks WHERE inode_id = ? AND block_index = ?` |
| `unlink` | Delete a file | `DELETE FROM inodes WHERE inode_id = ?` |

### 2. Why this is powerful for your "Agentic" vision

By storing the filesystem in SQLite, you gain features that no traditional filesystem possesses:

* **Complex Analytics:** You can ask questions that are impossible on standard drives.
* *Example:* "Find all files created by the 'agent' user that are larger than 1MB and contain the word 'config' in the name."
* *SQL:* `SELECT * FROM inodes WHERE owner = 'agent' AND size > 1024*1024 AND name LIKE '%config%';`


* **Version History:** Because you have a `layer_id`, you can perform "Time Travel" queries. You can compare two layers to see exactly what changed:
* *SQL:* `SELECT name FROM inodes WHERE layer_id = 'active' EXCEPT SELECT name FROM inodes WHERE layer_id = 'base';` (This shows you exactly what the agent added or renamed).


* **Atomic Integrity:** If the system crashes mid-write, you don't end up with a "corrupted filesystem" (which usually requires `fsck`). You just have an uncommitted SQLite transaction. The DB rolls back to the last clean state automatically.

### 3. The "Block" Storage Strategy

One of your design choices—storing raw data in a `blocks` table—is the most critical for performance.

* **The Problem:** If you store a whole 1GB file in a single SQL cell, SQLite will struggle to load that into memory, and you'll hit massive latency.
* **The Solution (Chunking):** You break the file into 4KB segments (or 64KB, depending on your performance testing).
* To read 10KB of a file, your FUSE driver asks for `block_index` 0 and 1.
* This allows for **Random Access**. If an agent only needs the last 100 bytes of a 50MB log file, you only fetch the specific block containing those bytes.



### 4. The Engineering "Gotcha": The Path Problem

There is one structural challenge you will face: **Path Resolution.**

POSIX works on paths like `/home/user/docs/file.txt`. SQLite works on IDs like `105`.
To turn a path into an ID, your code will have to recursively resolve the path:

1. Query `inodes` for `name='home', parent_id=0` (root).
2. Get `inode_id` for 'home'.
3. Query `inodes` for `name='user', parent_id=101`.
4. And so on.

**Pro-tip for your implementation:** This is exactly why you need the **Path Cache**. You will create an in-memory hash map: `HashMap<Path, InodeID>`. You check this map first; if it's not there, you do the SQL recursive lookup, then add the result to the cache.
