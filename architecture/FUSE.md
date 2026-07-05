### What is FUSE?

Normally, filesystems (like ext4, APFS, or NTFS) live entirely inside the kernel. The kernel has direct, highly privileged access to the hardware (the hard drive). When a user types `ls` or opens a text file, the system call goes straight into the kernel, which looks at the raw disk blocks and hands the data back.

Writing code in the kernel is dangerous. A single bug can cause a kernel panic and crash the entire machine. It's also notoriously hard to debug.

**FUSE** was created to solve this. It is a kernel module that acts as a bridge. It tells the kernel: *"Hey, pretend there is a normal filesystem mounted here. But whenever a program asks to read, write, or list files, don't go to the disk. Instead, package that request up and send it to this regular, user-level program."*

### The FUSE Data Flow

To visualize FUSE for LayeredFS, imagine the lifecycle of a command:

1. **The User/Agent:** Runs `cat /layer1/hello.txt`.
2. **The VFS (Virtual File System):** The Linux kernel receives the `read` system call and checks where `/layer1` is mounted. It sees it's a FUSE mount.
3. **The FUSE Kernel Module:** The kernel pauses the `cat` command. It takes the request ("read X bytes from inode Y") and pushes it through a special device file (`/dev/fuse`).
4. **Your Rust Program (LayeredFS):** Your program sits in a continuous `while` loop, listening to `/dev/fuse`. It receives the request.
5. **The Translation:** This is where your DB knowledge kicks in. Your Rust code looks at the FUSE request and translates it into: `SELECT data FROM blocks WHERE inode_id = Y`.
6. **The Response:** SQLite returns the binary data. Your Rust program sends it back to `/dev/fuse`.
7. **The Kernel:** Gives the data to `cat`, which prints it to the screen.

### Why FUSE is Both Magic and a Bottleneck

Because FUSE operates in userspace, you get to use standard libraries, talk to SQLite, and write in Rust without worrying about crashing the whole computer.

However, the candor of systems design requires pointing out the primary cost: **Context Switching**.
Every single time an application interacts with a file, the CPU must switch from User Mode -> Kernel Mode (FUSE) -> User Mode (Your Rust App) -> Kernel Mode (FUSE response) -> User Mode (The original app).
