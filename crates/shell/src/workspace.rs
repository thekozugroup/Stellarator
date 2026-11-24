//! One scratch folder per sample.
//!
//! ```text
//! <base>/                 /var/tmp/stellarator (Linux), /private/tmp/stellarator (macOS)
//!   .locks/<n>            held (flock, LockFileEx) while slot <n> is in use
//!   <n>/                  one sample; deleted when its Workspace is dropped
//!     workspace/          the commands' working directory, and the file helpers' root
//!     home/               HOME
//!     tmp/                TMPDIR
//! ```
//!
//! Slots are the lowest free numbers, so the paths a model sees are a small fixed set
//! (`/var/tmp/stellarator/3/workspace`) with no user name in them. The lock is the operating
//! system's and goes with the process: a slot whose lock can be taken is free, and a folder found in
//! it is left over from a process that died and is deleted first.

use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::{Error, IoContext as _};
use crate::files::FileLimits;

/// Slots tried before giving up. Far above any number of samples in flight.
const MAX_SLOTS: u32 = 4096;
/// Entries a template may have.
const MAX_TEMPLATE_ENTRIES: u64 = 200_000;

/// How a workspace starts and how large it may grow.
#[derive(Debug, Clone)]
pub struct WorkspaceOptions {
    /// A folder to copy into the workspace, or `None` to start empty. Symlinks are copied as
    /// symlinks; devices, sockets and pipes are left out.
    pub template: Option<PathBuf>,
    /// The size cap, in bytes, of the sample's folder (workspace, home and tmp together). A
    /// template over it is refused, the file helpers refuse to write past it, and a command that
    /// grows the folder past it is stopped at the next check (twice a second).
    pub max_bytes: u64,
    /// The file helpers' caps.
    pub files: FileLimits,
}

impl Default for WorkspaceOptions {
    fn default() -> Self {
        Self { template: None, max_bytes: 1 << 30, files: FileLimits::default() }
    }
}

/// The folder that holds the workspaces.
#[derive(Debug, Clone)]
pub struct Workspaces {
    base: PathBuf,
}

impl Workspaces {
    /// Where workspaces go on this platform.
    ///
    /// - Linux: `/var/tmp/stellarator`. Not `/tmp`, which is memory on Debian 13 and Fedora; not
    ///   `$TMPDIR` or the cache folder, whose paths hold the user's name.
    /// - macOS: `/private/tmp/stellarator`, the real path of `/tmp`, so that `pwd` and `pwd -P`
    ///   agree. `$TMPDIR` there is a long per-user path under `/var/folders`.
    /// - Windows: `%ProgramData%\Stellarator\workspaces`.
    pub fn default_base() -> PathBuf {
        #[cfg(target_os = "macos")]
        {
            PathBuf::from("/private/tmp/stellarator")
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            PathBuf::from("/var/tmp/stellarator")
        }
        #[cfg(windows)]
        {
            match std::env::var_os("ProgramData") {
                Some(dir) if !dir.is_empty() => PathBuf::from(dir).join("Stellarator").join("workspaces"),
                _ => std::env::temp_dir().join("stellarator-workspaces"),
            }
        }
    }

    /// Opens the default folder. When another user of this computer owns it, a folder named after
    /// this user's number is used instead (`/var/tmp/stellarator-1001`).
    pub fn open_default() -> Result<Self, Error> {
        let base = Self::default_base();
        match Self::open(&base) {
            Err(Error::Base { .. }) if cfg!(unix) => {
                let mut name = base.file_name().unwrap_or_default().to_os_string();
                name.push(format!("-{}", current_uid()));
                Self::open(base.with_file_name(name))
            }
            other => other,
        }
    }

    /// Opens `base`, creating it, and deletes the workspaces a dead process left in it.
    pub fn open(base: impl AsRef<Path>) -> Result<Self, Error> {
        let base = base.as_ref();
        let refuse = |reason: String| Error::Base { base: base.to_path_buf(), reason };
        if !base.is_absolute() {
            return Err(refuse("the path must be absolute".into()));
        }
        create_private_dir_all(base).map_err(|e| refuse(e.to_string()))?;
        // The real path: commands are given it as their working directory, and the sandbox's rules
        // name it.
        let base = fs::canonicalize(base).map_err(|e| refuse(e.to_string()))?;
        let base = strip_verbatim(base);
        check_owner(&base).map_err(refuse)?;
        create_private_dir_all(&base.join(".locks")).map_err(|e| refuse(e.to_string()))?;
        let workspaces = Self { base };
        workspaces.sweep();
        Ok(workspaces)
    }

    /// The folder, as a real path.
    pub fn base(&self) -> &Path {
        &self.base
    }

    /// Deletes the workspaces no live process holds, and returns how many there were.
    pub fn sweep(&self) -> usize {
        let Ok(entries) = fs::read_dir(&self.base) else { return 0 };
        let mut removed = 0;
        for entry in entries.flatten() {
            let Some(slot) = entry.file_name().to_str().and_then(|n| n.parse::<u32>().ok()) else { continue };
            let Ok(Some(_lock)) = self.lock(slot) else { continue };
            if remove_tree(&entry.path()).is_ok() {
                removed += 1;
            }
        }
        removed
    }

    /// Takes the lowest free slot and makes its folders.
    pub fn create(&self, options: &WorkspaceOptions) -> Result<Workspace, Error> {
        let template = match &options.template {
            Some(path) => {
                let real = fs::canonicalize(path)
                    .map_err(|e| Error::Template(format!("{}: {e}", path.display())))?;
                if !real.is_dir() {
                    return Err(Error::Template(format!("{} is not a folder", path.display())));
                }
                Some(real)
            }
            None => None,
        };
        for slot in 1..=MAX_SLOTS {
            let Some(lock) = self.lock(slot)? else { continue };
            let root = self.base.join(slot.to_string());
            // Left by a process that died: it held this lock, and we have it now.
            if root.symlink_metadata().is_ok() && remove_tree(&root).is_err() {
                continue;
            }
            let made = (|| {
                create_private_dir(&root)?;
                for name in ["workspace", "home", "tmp"] {
                    create_private_dir(&root.join(name))?;
                }
                Ok(())
            })();
            if let Err(source) = made {
                let _ = remove_tree(&root);
                return Err(Error::io(format!("Creating the workspace {}", root.display()), source));
            }
            let dir = root.join("workspace");
            let handle = cap_std::fs::Dir::open_ambient_dir(&dir, cap_std::ambient_authority())
                .context(|| format!("Opening the workspace {}", dir.display()));
            let handle = match handle {
                Ok(handle) => handle,
                Err(e) => {
                    let _ = remove_tree(&root);
                    return Err(e);
                }
            };
            let workspace = Workspace {
                slot,
                home: root.join("home"),
                tmp: root.join("tmp"),
                dir,
                root,
                max_bytes: options.max_bytes,
                files: options.files.clone(),
                measured: AtomicU64::new(0),
                handle,
                lock: Some(lock),
            };
            if let Some(template) = &template {
                // Dropping `workspace` on an error deletes what was copied.
                copy_tree(template, &workspace.dir, options.max_bytes)?;
            }
            workspace.measure();
            return Ok(workspace);
        }
        Err(Error::Base {
            base: self.base.clone(),
            reason: format!("all {MAX_SLOTS} workspace slots are in use"),
        })
    }

    /// The slot's lock, or `None` when a live process (or another thread of this one) holds it.
    fn lock(&self, slot: u32) -> Result<Option<File>, Error> {
        let path = self.base.join(".locks").join(slot.to_string());
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .context(|| format!("Opening the lock file {}", path.display()))?;
        match file.try_lock() {
            Ok(()) => Ok(Some(file)),
            Err(fs::TryLockError::WouldBlock) => Ok(None),
            Err(fs::TryLockError::Error(e)) => Err(Error::io(format!("Locking {}", path.display()), e)),
        }
    }
}

/// One sample's folder. Dropping it deletes the folder; [`Workspace::close`] does the same and
/// reports a failure.
#[derive(Debug)]
pub struct Workspace {
    slot: u32,
    /// `<base>/<n>`: everything a command may write.
    pub(crate) root: PathBuf,
    /// `<base>/<n>/workspace`.
    pub(crate) dir: PathBuf,
    pub(crate) home: PathBuf,
    pub(crate) tmp: PathBuf,
    pub(crate) max_bytes: u64,
    pub(crate) files: FileLimits,
    /// The folder's size at the last measurement.
    pub(crate) measured: AtomicU64,
    /// `dir`, opened once: the file helpers resolve every path beneath it.
    pub(crate) handle: cap_std::fs::Dir,
    lock: Option<File>,
}

impl Workspace {
    /// The commands' working directory and the file helpers' root. This is the path a model sees.
    pub fn path(&self) -> &Path {
        &self.dir
    }

    /// `HOME` of the commands.
    pub fn home(&self) -> &Path {
        &self.home
    }

    /// `TMPDIR` of the commands.
    pub fn tmp(&self) -> &Path {
        &self.tmp
    }

    /// The slot number, the one part of the path that differs between samples in flight.
    pub fn slot(&self) -> u32 {
        self.slot
    }

    /// The size cap in bytes.
    pub fn max_bytes(&self) -> u64 {
        self.max_bytes
    }

    /// Measures the sample's folder (workspace, home and tmp): the bytes its files take on disk.
    pub fn measure(&self) -> u64 {
        let size = tree_size(&self.root);
        self.measured.store(size, Ordering::Relaxed);
        size
    }

    /// Deletes the folder now and frees the slot.
    pub fn close(mut self) -> Result<(), Error> {
        self.remove()
    }

    fn remove(&mut self) -> Result<(), Error> {
        let Some(lock) = self.lock.take() else { return Ok(()) };
        let removed = remove_tree(&self.root).context(|| format!("Deleting the workspace {}", self.root.display()));
        // Released after the delete: a slot is never handed out while its old folder is going.
        drop(lock);
        removed
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = self.remove();
    }
}

// ---- Folders ---------------------------------------------------------------------------------

fn current_uid() -> u32 {
    #[cfg(unix)]
    {
        // SAFETY: geteuid has no failure mode and touches no memory.
        unsafe { libc::geteuid() }
    }
    #[cfg(not(unix))]
    {
        0
    }
}

/// `\\?\C:\...` as `C:\...`: `canonicalize` on Windows returns the verbatim form.
fn strip_verbatim(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(text) = path.to_str() {
            if let Some(rest) = text.strip_prefix(r"\\?\") {
                if !rest.starts_with("UNC\\") {
                    return PathBuf::from(rest);
                }
            }
        }
    }
    path
}

/// A folder only its owner can enter. On Windows it takes the parent's access list.
fn create_private_dir(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt as _;
        fs::DirBuilder::new().mode(0o700).create(path)
    }
    #[cfg(not(unix))]
    {
        fs::create_dir(path)
    }
}

fn create_private_dir_all(path: &Path) -> std::io::Result<()> {
    if path.is_dir() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        // Parents (`/var/tmp`, `%ProgramData%\Stellarator`) keep their own permissions.
        fs::create_dir_all(parent)?;
    }
    match create_private_dir(path) {
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && path.is_dir() => Ok(()),
        other => other,
    }
}

/// The base must be a real folder of this user that nobody else can write: another user who owned
/// it could swap a workspace for a link to somewhere else.
fn check_owner(base: &Path) -> Result<(), String> {
    let meta = base.symlink_metadata().map_err(|e| e.to_string())?;
    if !meta.is_dir() {
        return Err("it is not a folder".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
        if meta.uid() != current_uid() {
            return Err(format!("it belongs to another user (uid {})", meta.uid()));
        }
        if meta.mode() & 0o077 != 0 {
            fs::set_permissions(base, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// The bytes the files under `path` take on disk. Files that vanish while it walks are skipped.
pub(crate) fn tree_size(path: &Path) -> u64 {
    let mut total = 0u64;
    let mut pending = vec![path.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            total = total.saturating_add(disk_bytes(&meta));
            if meta.is_dir() {
                pending.push(entry.path());
            }
        }
    }
    total
}

fn disk_bytes(meta: &fs::Metadata) -> u64 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        // Blocks, not length: a sparse file takes what it has written.
        meta.blocks().saturating_mul(512)
    }
    #[cfg(not(unix))]
    {
        meta.len()
    }
}

/// Deletes a folder a command may have left in any state: folders without permissions, and on
/// macOS files flagged immutable.
pub(crate) fn remove_tree(path: &Path) -> std::io::Result<()> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => {
            make_removable(path);
            match fs::remove_dir_all(path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                other => other,
            }
        }
    }
}

/// Gives the owner back the permissions `remove_dir_all` needs. Symlinks are never followed.
fn make_removable(path: &Path) {
    let mut pending = vec![path.to_path_buf()];
    while let Some(current) = pending.pop() {
        let Ok(meta) = current.symlink_metadata() else { continue };
        if meta.file_type().is_symlink() {
            continue;
        }
        #[cfg(target_os = "macos")]
        clear_flags(&current);
        #[cfg(windows)]
        {
            let mut permissions = meta.permissions();
            if permissions.readonly() {
                #[allow(clippy::permissions_set_readonly_false)]
                permissions.set_readonly(false);
                let _ = fs::set_permissions(&current, permissions);
            }
        }
        if meta.is_dir() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt as _;
                let _ = fs::set_permissions(&current, fs::Permissions::from_mode(0o700));
            }
            if let Ok(entries) = fs::read_dir(&current) {
                pending.extend(entries.flatten().map(|e| e.path()));
            }
        }
    }
}

/// `chflags(path, 0)`: a command can set the user-immutable flag on its own files.
#[cfg(target_os = "macos")]
fn clear_flags(path: &Path) {
    use std::os::unix::ffi::OsStrExt as _;
    if let Ok(c_path) = std::ffi::CString::new(path.as_os_str().as_bytes()) {
        // SAFETY: a valid NUL-terminated path; lchflags does not follow a symlink.
        unsafe { libc::lchflags(c_path.as_ptr(), 0) };
    }
}

/// Copies the template into an empty workspace, refusing one over `max_bytes`.
fn copy_tree(from: &Path, to: &Path, max_bytes: u64) -> Result<(), Error> {
    let too_large = || Error::Template(format!("{} is larger than the workspace's {max_bytes} bytes", from.display()));
    let failed = |path: &Path, e: std::io::Error| Error::Template(format!("{}: {e}", path.display()));
    let mut bytes = 0u64;
    let mut count = 0u64;
    let mut pending = vec![(from.to_path_buf(), to.to_path_buf())];
    while let Some((source_dir, target_dir)) = pending.pop() {
        for entry in fs::read_dir(&source_dir).map_err(|e| failed(&source_dir, e))? {
            let entry = entry.map_err(|e| failed(&source_dir, e))?;
            let source = entry.path();
            let target = target_dir.join(entry.file_name());
            let meta = source.symlink_metadata().map_err(|e| failed(&source, e))?;
            count += 1;
            if count > MAX_TEMPLATE_ENTRIES {
                return Err(Error::Template(format!(
                    "{} has more than {MAX_TEMPLATE_ENTRIES} files and folders",
                    from.display()
                )));
            }
            let kind = meta.file_type();
            if kind.is_dir() {
                create_private_dir(&target).map_err(|e| failed(&target, e))?;
                pending.push((source, target));
            } else if kind.is_file() {
                bytes = bytes.saturating_add(meta.len());
                if bytes > max_bytes {
                    return Err(too_large());
                }
                // Keeps the permission bits, so scripts stay executable.
                fs::copy(&source, &target).map_err(|e| failed(&source, e))?;
            } else if kind.is_symlink() {
                #[cfg(unix)]
                {
                    let link = fs::read_link(&source).map_err(|e| failed(&source, e))?;
                    std::os::unix::fs::symlink(link, &target).map_err(|e| failed(&target, e))?;
                }
            }
            // Devices, sockets and pipes are left out.
        }
    }
    Ok(())
}
