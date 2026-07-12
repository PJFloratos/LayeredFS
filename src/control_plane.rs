use crate::database::LayeredDb;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// Spawns the interactive CLI control plane in a background thread.
pub fn spawn(db: Arc<Mutex<LayeredDb>>) {
    thread::spawn(move || {
        let stdin = io::stdin();
        // Give FUSE a second to print its startup text before showing the prompt
        thread::sleep(Duration::from_millis(500));

        loop {
            print!("\nLayeredFS> ");
            io::stdout().flush().unwrap();

            let mut input = String::new();
            if stdin.read_line(&mut input).is_ok() {
                let cmd = input.trim();

                if cmd.starts_with("commit ") {
                    let commit_name = cmd.trim_start_matches("commit ").trim();
                    if commit_name.is_empty() {
                        println!("Error: Commit name cannot be empty.");
                        continue;
                    }

                    // Lock the DB and run the commit
                    let db_lock = db.lock().unwrap();
                    match db_lock.commit(commit_name) {
                        Ok(_) => {
                            println!("SUCCESS: Filesystem state committed as '{}'", commit_name)
                        }
                        Err(e) => println!("ERROR: Commit failed: {}", e),
                    }
                } else if cmd.starts_with("sql ") {
                    let query = cmd.trim_start_matches("sql ").trim();
                    if query.is_empty() {
                        println!("Error: SQL query cannot be empty.");
                        continue;
                    }

                    let db_lock = db.lock().unwrap();
                    match db_lock.execute_raw_sql(query) {
                        Ok(output) => {
                            for line in output {
                                println!("{}", line);
                            }
                        }
                        Err(e) => println!("ERROR: SQL execution failed: {}", e),
                    }
                } else if cmd == "reset" {
                    print!("Are you sure you want to completely wipe the filesystem? (y/N): ");
                    io::stdout().flush().unwrap();

                    let mut confirmation = String::new();
                    if stdin.read_line(&mut confirmation).is_ok()
                        && confirmation.trim().to_lowercase() == "y"
                    {
                        let db_lock = db.lock().unwrap();

                        println!("Wiping database tables...");
                        if let Err(e) = db_lock.wipe_all_data() {
                            println!("ERROR: Failed to wipe data: {}", e);
                            continue;
                        }

                        println!("Re-bootstrapping fresh schema and root inode...");
                        match db_lock.bootstrap_initial_state() {
                            Ok(_) => println!("SUCCESS: Filesystem has been completely reset to factory defaults!"),
                            Err(e) => println!("ERROR: Re-bootstrap failed: {}", e),
                        }
                    } else {
                        println!("Reset aborted.");
                    }
                } else if cmd == "help" {
                    println!("Available commands:");
                    println!("  commit <name>   - Freezes the active layer and starts a new one.");
                    println!("  sql <query>     - Executes raw SQL against the database.");
                    println!("  reset           - Wipes all data and returns to a clean slate.");
                } else if !cmd.is_empty() {
                    println!("Unknown command. Type 'help'.");
                }
            }
        }
    });
}
