//! Landlock: the file system rules. Called through the three system calls directly, so that the
//! ruleset can be made in the parent and enforced in the child with one call that allocates nothing.
//!
//! Landlock is an allowlist: a sandboxed process may open only what a rule names. It does not
//! cover `stat`, `chmod`, `chown`, `utimes` or extended attributes (seccomp and the supervisor in
//! `notify.rs` cover those), and before ABI 6 it does not cover signals.

use std::ffi::CString;
use std::io;
use std::os::fd::{AsRawFd as _, FromRawFd as _, OwnedFd, RawFd};
use std::os::unix::ffi::OsStrExt as _;
use std::path::Path;

/// The oldest ABI the sandbox accepts: 3 (Linux 6.2) is the first that covers `truncate`, and 2
/// (Linux 5.19) the first where a rename between two folders of the workspace works.
pub(crate) const MIN_ABI: u32 = 3;
/// The newest ABI this code knows. A newer kernel is used as this one.
pub(crate) const MAX_ABI: u32 = 6;

const CREATE_RULESET_VERSION: libc::c_ulong = 1;
const RULE_PATH_BENEATH: libc::c_int = 1;

pub(crate) const FS_EXECUTE: u64 = 1 << 0;
pub(crate) const FS_WRITE_FILE: u64 = 1 << 1;
pub(crate) const FS_READ_FILE: u64 = 1 << 2;
pub(crate) const FS_READ_DIR: u64 = 1 << 3;
pub(crate) const FS_REMOVE_DIR: u64 = 1 << 4;
pub(crate) const FS_REMOVE_FILE: u64 = 1 << 5;
pub(crate) const FS_MAKE_CHAR: u64 = 1 << 6;
pub(crate) const FS_MAKE_DIR: u64 = 1 << 7;
pub(crate) const FS_MAKE_REG: u64 = 1 << 8;
pub(crate) const FS_MAKE_SOCK: u64 = 1 << 9;
pub(crate) const FS_MAKE_FIFO: u64 = 1 << 10;
pub(crate) const FS_MAKE_BLOCK: u64 = 1 << 11;
pub(crate) const FS_MAKE_SYM: u64 = 1 << 12;
pub(crate) const FS_REFER: u64 = 1 << 13; // ABI 2
pub(crate) const FS_TRUNCATE: u64 = 1 << 14; // ABI 3
pub(crate) const FS_IOCTL_DEV: u64 = 1 << 15; // ABI 5

const NET_BIND_TCP: u64 = 1 << 0; // ABI 4
const NET_CONNECT_TCP: u64 = 1 << 1;
const SCOPE_ABSTRACT_UNIX_SOCKET: u64 = 1 << 0; // ABI 6
const SCOPE_SIGNAL: u64 = 1 << 1;

/// The rights that make sense on a file that is not a folder; a rule on a file with any other
/// right is refused by the kernel.
const FILE_RIGHTS: u64 = FS_EXECUTE | FS_WRITE_FILE | FS_READ_FILE | FS_TRUNCATE | FS_IOCTL_DEV;

#[repr(C)]
struct RulesetAttr {
    handled_access_fs: u64,
    handled_access_net: u64,
    scoped: u64,
}

#[repr(C, packed)]
struct PathBeneathAttr {
    allowed_access: u64,
    parent_fd: i32,
}

/// The kernel's Landlock ABI, or why there is none (`ENOSYS`: not built in; `EOPNOTSUPP`: built in
/// and turned off at boot).
pub(crate) fn kernel_abi() -> io::Result<u32> {
    // SAFETY: a null attribute with the version flag only asks for the ABI number.
    let abi = unsafe {
        libc::syscall(libc::SYS_landlock_create_ruleset, std::ptr::null::<RulesetAttr>(), 0usize, CREATE_RULESET_VERSION)
    };
    if abi < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(abi as u32)
}

/// Every file system right ABI `abi` knows.
pub(crate) fn all_fs(abi: u32) -> u64 {
    let mut rights = FS_EXECUTE
        | FS_WRITE_FILE
        | FS_READ_FILE
        | FS_READ_DIR
        | FS_REMOVE_DIR
        | FS_REMOVE_FILE
        | FS_MAKE_CHAR
        | FS_MAKE_DIR
        | FS_MAKE_REG
        | FS_MAKE_SOCK
        | FS_MAKE_FIFO
        | FS_MAKE_BLOCK
        | FS_MAKE_SYM
        | FS_REFER
        | FS_TRUNCATE;
    if abi >= 5 {
        rights |= FS_IOCTL_DEV;
    }
    rights
}

/// Read and run: the system folders.
pub(crate) const READ_EXEC: u64 = FS_EXECUTE | FS_READ_FILE | FS_READ_DIR;
/// Read only.
pub(crate) const READ: u64 = FS_READ_FILE | FS_READ_DIR;
/// Read and write an existing device node (`/dev/null`).
pub(crate) const DEVICE: u64 = FS_READ_FILE | FS_WRITE_FILE | FS_IOCTL_DEV;

/// Everything in the workspace except making device nodes.
pub(crate) fn workspace(abi: u32) -> u64 {
    all_fs(abi) & !(FS_MAKE_CHAR | FS_MAKE_BLOCK | FS_IOCTL_DEV)
}

/// A ruleset being filled in the parent.
pub(crate) struct Ruleset {
    fd: OwnedFd,
    handled_fs: u64,
}

impl Ruleset {
    /// A ruleset that handles every file system right of `abi`, TCP when `block_network` (ABI 4),
    /// and signals and abstract Unix sockets to processes outside (ABI 6).
    pub(crate) fn new(abi: u32, block_network: bool) -> io::Result<Self> {
        let handled_fs = all_fs(abi);
        let attr = RulesetAttr {
            handled_access_fs: handled_fs,
            handled_access_net: if abi >= 4 && block_network { NET_BIND_TCP | NET_CONNECT_TCP } else { 0 },
            scoped: if abi >= 6 { SCOPE_ABSTRACT_UNIX_SOCKET | SCOPE_SIGNAL } else { 0 },
        };
        // A kernel older than the attribute's last field takes the struct as long as the tail it
        // does not know is zero, which it is by construction above.
        // SAFETY: `attr` is a valid, initialized attribute of the size given.
        let fd = unsafe {
            libc::syscall(
                libc::SYS_landlock_create_ruleset,
                &attr as *const RulesetAttr,
                size_of::<RulesetAttr>(),
                0 as libc::c_long,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: the call returned a new descriptor that nothing else owns. It is close-on-exec.
        Ok(Self { fd: unsafe { OwnedFd::from_raw_fd(fd as RawFd) }, handled_fs })
    }

    /// Allows `access` on `path` and everything beneath it. A path that does not exist is skipped
    /// (`Ok(false)`). A symlink is followed: the rule lands on what it points at, which is what
    /// `/etc/resolv.conf` and `/bin` need.
    pub(crate) fn allow(&self, path: &Path, access: u64) -> io::Result<bool> {
        let Ok(c_path) = CString::new(path.as_os_str().as_bytes()) else { return Ok(false) };
        // SAFETY: a valid NUL-terminated path.
        let raw = unsafe { libc::open(c_path.as_ptr(), libc::O_PATH | libc::O_CLOEXEC) };
        if raw < 0 {
            let error = io::Error::last_os_error();
            return match error.raw_os_error() {
                Some(libc::ENOENT | libc::ENOTDIR | libc::EACCES | libc::ELOOP) => Ok(false),
                _ => Err(error),
            };
        }
        // SAFETY: a descriptor we just opened.
        let target = unsafe { OwnedFd::from_raw_fd(raw) };
        // SAFETY: fstat on a descriptor we own, into a zeroed stat.
        let is_dir = unsafe {
            let mut stat: libc::stat = std::mem::zeroed();
            libc::fstat(target.as_raw_fd(), &mut stat) == 0 && (stat.st_mode & libc::S_IFMT) == libc::S_IFDIR
        };
        let mut allowed = access & self.handled_fs;
        if !is_dir {
            allowed &= FILE_RIGHTS;
        }
        let attr = PathBeneathAttr { allowed_access: allowed, parent_fd: target.as_raw_fd() };
        // SAFETY: `attr` is valid for the call; both descriptors are open.
        let result = unsafe {
            libc::syscall(
                libc::SYS_landlock_add_rule,
                self.fd.as_raw_fd() as libc::c_long,
                RULE_PATH_BENEATH as libc::c_long,
                &attr as *const PathBeneathAttr,
                0 as libc::c_long,
            )
        };
        if result < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(true)
    }

    pub(crate) fn raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}

/// Enforces the ruleset on the calling thread, for good. `no_new_privs` must be set already.
///
/// # Safety
/// Async-signal-safe: one system call, no allocation. `ruleset` must be an open ruleset.
pub(crate) unsafe fn restrict_self(ruleset: RawFd) -> libc::c_long {
    libc::syscall(libc::SYS_landlock_restrict_self, ruleset as libc::c_long, 0 as libc::c_long)
}
