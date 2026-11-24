//! What Linux and macOS share: the shell, and the part of the child's setup that runs between
//! `fork` and `exec`.
//!
//! Code that runs there must be async-signal-safe: the process is a copy of a multi-threaded one,
//! and a lock another thread held at the fork (the allocator's, for one) is held forever in the
//! copy. So these functions make system calls on values prepared beforehand and do nothing else:
//! no allocation, no formatting, no `std::io::Error::new`.

use std::path::Path;
use std::sync::OnceLock;

use crate::run::Limits;

/// bash where there is one, else sh.
pub(crate) fn shell() -> &'static Path {
    static SHELL: OnceLock<&'static Path> = OnceLock::new();
    SHELL.get_or_init(|| {
        ["/bin/bash", "/usr/bin/bash", "/usr/local/bin/bash", "/bin/sh"]
            .into_iter()
            .map(Path::new)
            .find(|p| p.is_file())
            .unwrap_or(Path::new("/bin/sh"))
    })
}

pub(crate) fn errno() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(libc::EIO)
}

/// The resource limits of a command, ready to apply in the child.
#[derive(Clone, Copy)]
pub(crate) struct Rlimits {
    entries: [(libc::c_int, u64, u64); 6],
    count: usize,
}

impl Rlimits {
    /// `processes_now`: how many processes (Linux: threads) the user runs already, which
    /// `RLIMIT_NPROC` counts; `None` when the kernel would not apply the limit (root on Linux).
    pub(crate) fn new(limits: &Limits, processes_now: Option<u64>) -> Self {
        let mut list = Self { entries: [(0, 0, 0); 6], count: 0 };
        // A core file would be written into the workspace.
        list.push(libc::RLIMIT_CORE as libc::c_int, 0, 0);
        if let Some(seconds) = limits.cpu_seconds {
            // SIGXCPU at the soft limit says why; SIGKILL a second later for a process that
            // ignores it.
            list.push(libc::RLIMIT_CPU as libc::c_int, seconds, seconds.saturating_add(1));
        }
        if let Some(bytes) = limits.file_bytes {
            list.push(libc::RLIMIT_FSIZE as libc::c_int, bytes, bytes);
        }
        if let Some(files) = limits.open_files {
            list.push(libc::RLIMIT_NOFILE as libc::c_int, files, files);
        }
        if let (Some(extra), Some(now)) = (limits.processes, processes_now) {
            let total = now.saturating_add(extra);
            list.push(libc::RLIMIT_NPROC as libc::c_int, total, total);
        }
        // macOS does not keep RLIMIT_DATA or RLIMIT_AS for memory that malloc maps; the monitor
        // thread watches memory there.
        #[cfg(target_os = "linux")]
        if let Some(bytes) = limits.memory_bytes {
            list.push(libc::RLIMIT_DATA as libc::c_int, bytes, bytes);
        }
        list
    }

    fn push(&mut self, resource: libc::c_int, soft: u64, hard: u64) {
        self.entries[self.count] = (resource, soft, hard);
        self.count += 1;
    }

    /// Applies the limits to the calling process. A limit above what the process may set is
    /// lowered to that. Returns the `errno` of a call that failed.
    ///
    /// # Safety
    /// Async-signal-safe; meant for the child between `fork` and `exec`.
    pub(crate) unsafe fn apply(&self) -> Result<(), i32> {
        for (resource, soft, hard) in &self.entries[..self.count] {
            let mut current = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
            if libc::getrlimit(*resource as _, &mut current) != 0 {
                return Err(errno());
            }
            let hard = (*hard as libc::rlim_t).min(current.rlim_max);
            let wanted = libc::rlimit { rlim_cur: (*soft as libc::rlim_t).min(hard), rlim_max: hard };
            if libc::setrlimit(*resource as _, &wanted) != 0 {
                return Err(errno());
            }
        }
        Ok(())
    }
}

/// Starts a session of the command's own (so that one signal reaches its process group, and it has
/// no terminal), and makes a write past the file size limit an error instead of a signal.
///
/// # Safety
/// Async-signal-safe; meant for the child between `fork` and `exec`.
pub(crate) unsafe fn detach() -> Result<(), i32> {
    if libc::setsid() < 0 {
        return Err(errno());
    }
    // Ignored signals stay ignored across exec: writes then fail with EFBIG, which tools report.
    libc::signal(libc::SIGXFSZ, libc::SIG_IGN);
    Ok(())
}

/// Marks every descriptor above stderr close-on-exec, so the command inherits nothing Stellarator
/// has open. Rust opens its own files that way already; libraries may not.
///
/// # Safety
/// Async-signal-safe; meant for the child between `fork` and `exec`.
pub(crate) unsafe fn close_inherited() {
    #[cfg(target_os = "linux")]
    {
        // Linux 5.11; the sandbox needs 6.2.
        libc::syscall(
            libc::SYS_close_range,
            3 as libc::c_long,
            u32::MAX as libc::c_long,
            libc::CLOSE_RANGE_CLOEXEC as libc::c_long,
        );
    }
    #[cfg(not(target_os = "linux"))]
    {
        let mut limit = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
        let highest = if libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) == 0 {
            limit.rlim_cur.min(65_536) as libc::c_int
        } else {
            4096
        };
        for fd in 3..highest {
            let flags = libc::fcntl(fd, libc::F_GETFD);
            if flags >= 0 && flags & libc::FD_CLOEXEC == 0 {
                libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC);
            }
        }
    }
}
