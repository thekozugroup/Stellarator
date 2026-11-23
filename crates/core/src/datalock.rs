//! One process per data folder. Two processes writing one SQLite database (the desktop app opened
//! twice, the nsh app beside the Tauri app, a server started from a terminal on the app's folder)
//! would interleave runs and imports, so opening a data folder takes an exclusive lock on
//! `stellarator.lock` in it, held until the process ends. A second process gets
//! [`DataFolderInUse`] and says so instead of opening the database.
//!
//! The lock is the operating system's (`flock` on Unix, `LockFileEx` on Windows), so it goes with
//! the process, even when it crashes: a stale lock file never blocks a later start. Opening the
//! same folder again in the same process (tests, a retried start) reuses the lock.

use std::fs::{File, OpenOptions, TryLockError};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The lock file's name, in the data folder.
pub const LOCK_FILE: &str = "stellarator.lock";

/// Another process holds the data folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataFolderInUse {
    pub dir: PathBuf,
}

impl std::fmt::Display for DataFolderInUse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Another Stellarator is already using the data folder {}. Quit it (the desktop app, or a \
             server started from a terminal), then open Stellarator again.",
            self.dir.display()
        )
    }
}

impl std::error::Error for DataFolderInUse {}

/// The folders this process holds, with their open lock files.
static HELD: Mutex<Vec<(PathBuf, File)>> = Mutex::new(Vec::new());

/// Takes the lock on `dir` for the rest of the process. `Err` is [`DataFolderInUse`] when another
/// process holds it, or the I/O error that kept the lock file from opening. A file system without
/// locks (some network shares) is let through with a warning: refusing to start there would be
/// worse than the risk.
pub fn acquire(dir: &Path) -> anyhow::Result<()> {
    let dir = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    let mut held = HELD.lock().unwrap_or_else(|e| e.into_inner());
    if held.iter().any(|(d, _)| *d == dir) {
        return Ok(());
    }
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(LOCK_FILE);
    let mut file = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(&path)?;
    match file.try_lock() {
        Ok(()) => {
            // Who holds it, for someone looking at the folder. Readable only on Unix: Windows
            // locks are mandatory for other processes.
            let _ = file.set_len(0);
            let _ = writeln!(file, "pid {}", std::process::id());
            held.push((dir, file));
            Ok(())
        }
        Err(TryLockError::WouldBlock) => Err(DataFolderInUse { dir }.into()),
        Err(TryLockError::Error(e)) => {
            tracing::warn!(error = %e, path = %path.display(), "could not lock the data folder; opening it anyway");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A new empty folder under the system's temporary folder.
    fn temp_folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("stellarator-lock-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_same_process_opens_a_folder_twice() {
        let dir = temp_folder("twice");
        acquire(&dir).unwrap();
        acquire(&dir).unwrap();
        assert!(dir.join(LOCK_FILE).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn another_handle_finds_the_folder_locked() {
        let dir = temp_folder("other");
        acquire(&dir).unwrap();
        // A second open file description stands in for a second process: OS locks are per handle.
        let other = OpenOptions::new().read(true).write(true).open(dir.join(LOCK_FILE)).unwrap();
        assert!(matches!(other.try_lock(), Err(TryLockError::WouldBlock)));
        let message = DataFolderInUse { dir: dir.clone() }.to_string();
        assert!(message.starts_with("Another Stellarator is already using the data folder"));
        drop(other);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
