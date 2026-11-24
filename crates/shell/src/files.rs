//! File helpers confined to a workspace: read, write, edit and list.
//!
//! They run in Stellarator's own process, outside the sandbox, while a command of the same sample
//! may be running inside it. A command can plant a symlink to any path, or swap a folder for one
//! between a check and an open. So no path is checked first and opened afterwards: every path is
//! resolved beneath the workspace's open folder by `cap-std` (`openat2` with `RESOLVE_BENEATH` on
//! Linux, one component at a time elsewhere), which refuses `..` and symlinks that lead outside.

use std::io::{Read as _, Write as _};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::Ordering;

use cap_std::fs::OpenOptions;

use crate::error::Error;
use crate::workspace::Workspace;

/// Caps of the file helpers.
#[derive(Debug, Clone)]
pub struct FileLimits {
    /// The largest file `read`, `read_to_string` and `edit` take.
    pub max_read_bytes: u64,
    /// The largest content `write` and `edit` store.
    pub max_write_bytes: u64,
    /// The most entries `list` returns.
    pub max_list_entries: usize,
}

impl Default for FileLimits {
    fn default() -> Self {
        Self { max_read_bytes: 1 << 20, max_write_bytes: 1 << 20, max_list_entries: 2000 }
    }
}

/// What a folder entry is. A symlink is reported as a symlink, whatever it points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Dir,
    Symlink,
    Other,
}

/// One entry of a folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub name: String,
    pub kind: EntryKind,
    /// The length in bytes of a file; 0 for anything else.
    pub size: u64,
}

/// A folder's entries, sorted by name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listing {
    pub entries: Vec<DirEntry>,
    /// The folder has more entries than `max_list_entries`; the rest are left out.
    pub truncated: bool,
}

impl Workspace {
    /// Reads a file. `path` is relative to the workspace, or absolute inside it.
    pub fn read(&self, path: impl AsRef<Path>) -> Result<Vec<u8>, Error> {
        let path = path.as_ref();
        let relative = self.relative(path)?;
        let mut options = OpenOptions::new();
        options.read(true);
        let file = self.open(path, &relative, &mut options)?;
        let limit = self.files.max_read_bytes;
        let size = file.metadata().map_err(|e| self.file_error(path, e))?.len();
        if size > limit {
            return Err(Error::TooLarge { what: path.display().to_string(), size, limit });
        }
        let mut contents = Vec::with_capacity(size as usize);
        // One byte past the cap tells a file that grew since the size was read.
        file.take(limit + 1).read_to_end(&mut contents).map_err(|e| self.file_error(path, e))?;
        if contents.len() as u64 > limit {
            return Err(Error::TooLarge { what: path.display().to_string(), size: contents.len() as u64, limit });
        }
        Ok(contents)
    }

    /// Reads a file of UTF-8 text.
    pub fn read_to_string(&self, path: impl AsRef<Path>) -> Result<String, Error> {
        let path = path.as_ref();
        String::from_utf8(self.read(path)?).map_err(|_| Error::NotText { path: path.to_path_buf() })
    }

    /// Writes a file, replacing it if it exists and creating the folders above it.
    pub fn write(&self, path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> Result<(), Error> {
        let (path, contents) = (path.as_ref(), contents.as_ref());
        let relative = self.relative(path)?;
        let size = contents.len() as u64;
        if size > self.files.max_write_bytes {
            return Err(Error::TooLarge {
                what: format!("The content for {}", path.display()),
                size,
                limit: self.files.max_write_bytes,
            });
        }
        // The estimate first; a real measurement only when the estimate says no.
        if self.measured.load(Ordering::Relaxed).saturating_add(size) > self.max_bytes {
            let used = self.measure();
            if used.saturating_add(size) > self.max_bytes {
                return Err(Error::WorkspaceFull { size: used, limit: self.max_bytes });
            }
        }
        if let Some(parent) = relative.parent().filter(|p| !p.as_os_str().is_empty()) {
            match self.handle.create_dir_all(parent) {
                Ok(()) => {}
                // Something is there that is not a folder inside the workspace (a symlink out of
                // it, a file): opening the path below says which, and refuses it.
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(self.file_error(path, e)),
            }
        }
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        let mut file = self.open(path, &relative, &mut options)?;
        file.write_all(contents).map_err(|e| self.file_error(path, e))?;
        self.measured.fetch_add(size, Ordering::Relaxed);
        Ok(())
    }

    /// Replaces `old` with `new` in a text file. `old` must be in the file exactly once:
    /// [`Error::NoMatch`] when it is not there (or empty), [`Error::MultipleMatches`] when it is
    /// there several times.
    pub fn edit(&self, path: impl AsRef<Path>, old: &str, new: &str) -> Result<(), Error> {
        let path = path.as_ref();
        let text = self.read_to_string(path)?;
        let count = if old.is_empty() { 0 } else { text.matches(old).count() };
        match count {
            0 => Err(Error::NoMatch { path: path.to_path_buf() }),
            1 => self.write(path, text.replacen(old, new, 1)),
            count => Err(Error::MultipleMatches { path: path.to_path_buf(), count }),
        }
    }

    /// Lists a folder (`.` or an empty path for the workspace itself).
    pub fn list(&self, path: impl AsRef<Path>) -> Result<Listing, Error> {
        let path = path.as_ref();
        let relative = if path.as_os_str().is_empty() { PathBuf::from(".") } else { self.relative(path)? };
        let read_dir = self.handle.read_dir(&relative).map_err(|e| {
            let error = self.file_error(path, e);
            match error {
                Error::Io { .. } if self.handle.metadata(&relative).is_ok_and(|m| !m.is_dir()) => {
                    Error::WrongKind { path: path.to_path_buf(), expected: "a folder" }
                }
                other => other,
            }
        })?;
        // Every name first, so that the entries kept are the first by name; a folder with more
        // than this many is cut where the system stops.
        const MOST_NAMES: usize = 100_000;
        let mut found = Vec::new();
        for entry in read_dir {
            let entry = entry.map_err(|e| self.file_error(path, e))?;
            found.push((entry.file_name().to_string_lossy().into_owned(), entry));
            if found.len() >= MOST_NAMES {
                break;
            }
        }
        found.sort_by(|a, b| a.0.cmp(&b.0));
        let truncated = found.len() > self.files.max_list_entries;
        found.truncate(self.files.max_list_entries);
        let entries = found
            .into_iter()
            .map(|(name, entry)| {
                let kind = match entry.file_type() {
                    Ok(kind) if kind.is_symlink() => EntryKind::Symlink,
                    Ok(kind) if kind.is_dir() => EntryKind::Dir,
                    Ok(kind) if kind.is_file() => EntryKind::File,
                    _ => EntryKind::Other,
                };
                let size = match kind {
                    EntryKind::File => entry.metadata().map(|m| m.len()).unwrap_or(0),
                    _ => 0,
                };
                DirEntry { name, kind, size }
            })
            .collect();
        Ok(Listing { entries, truncated })
    }

    /// `path` relative to the workspace. Refuses what leaves it by its spelling alone; symlinks are
    /// caught when the path is opened.
    fn relative(&self, path: &Path) -> Result<PathBuf, Error> {
        let outside = || Error::OutsideWorkspace { path: path.to_path_buf() };
        if path.as_os_str().is_empty() {
            return Err(Error::InvalidPath("the path is empty".into()));
        }
        if path.as_os_str().as_encoded_bytes().contains(&0) {
            return Err(Error::InvalidPath("the path has a NUL character".into()));
        }
        let inside = if path.is_absolute() { path.strip_prefix(&self.dir).map_err(|_| outside())? } else { path };
        let mut depth = 0usize;
        let mut relative = PathBuf::new();
        for component in inside.components() {
            match component {
                Component::Normal(name) => {
                    depth += 1;
                    relative.push(name);
                }
                Component::ParentDir => {
                    depth = depth.checked_sub(1).ok_or_else(outside)?;
                    relative.push("..");
                }
                Component::CurDir => {}
                // A drive or a second root: not beneath the workspace.
                Component::Prefix(_) | Component::RootDir => return Err(outside()),
            }
        }
        if relative.as_os_str().is_empty() {
            relative.push(".");
        }
        Ok(relative)
    }

    /// Opens a regular file beneath the workspace.
    fn open(&self, path: &Path, relative: &Path, options: &mut OpenOptions) -> Result<cap_std::fs::File, Error> {
        #[cfg(unix)]
        {
            use cap_std::fs::OpenOptionsExt as _;
            // A command can put a named pipe where a file is expected; opening one would wait for
            // its other end forever.
            options.custom_flags(libc::O_NONBLOCK);
        }
        let file = self.handle.open_with(relative, options).map_err(|e| {
            let error = self.file_error(path, e);
            match error {
                // Windows answers "access denied" for a folder opened as a file.
                Error::Io { .. } if self.handle.metadata(relative).is_ok_and(|m| m.is_dir()) => {
                    Error::WrongKind { path: path.to_path_buf(), expected: "a file" }
                }
                other => other,
            }
        })?;
        let meta = file.metadata().map_err(|e| self.file_error(path, e))?;
        if !meta.is_file() {
            return Err(Error::WrongKind { path: path.to_path_buf(), expected: "a file" });
        }
        Ok(file)
    }

    fn file_error(&self, path: &Path, error: std::io::Error) -> Error {
        // cap-std's own error for a path that led outside; the system's "permission denied" has
        // an OS error code.
        if error.kind() == std::io::ErrorKind::PermissionDenied && error.raw_os_error().is_none() {
            return Error::OutsideWorkspace { path: path.to_path_buf() };
        }
        if error.kind() == std::io::ErrorKind::IsADirectory {
            return Error::WrongKind { path: path.to_path_buf(), expected: "a file" };
        }
        Error::io(path.display().to_string(), error)
    }
}
