//! The supervisor: Stellarator's side of the seccomp filter's "ask" answers.
//!
//! The filter stops a few system calls and hands them to a listener descriptor that the child sent
//! over a socket before it started the shell. One thread per command reads them here.
//!
//! - **`chmod`, `chown`, `utimes`.** Landlock does not cover them, so a command could change the
//!   mode or dates of any file its user owns. The supervisor resolves the file the call names into
//!   a descriptor of its own, checks that this descriptor is inside the sample's folder, and makes
//!   the change itself on that descriptor. The call never runs in the command's process, so the
//!   command cannot swap the path between the check and the change.
//! - **`setsid`.** Allowed, and the new session's number is recorded. A process can leave its
//!   process group but not its session without this call, so the recorded sessions hold every
//!   process of the command, and killing them leaves none.
//! - **Signals**, where the kernel's Landlock is older than ABI 6: allowed to processes of the
//!   command's sessions only.
//!
//! When the last process of the command is gone the listener reports a hang-up; that is how the
//! runner knows the tree is empty without looking.

use std::collections::HashSet;
use std::ffi::CString;
use std::os::fd::{AsRawFd as _, FromRawFd as _, OwnedFd, RawFd};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use libc::{c_long, seccomp_notif, seccomp_notif_resp};

use super::{procfs, seccomp};

// _IOWR('!', 0, struct seccomp_notif), _IOWR('!', 1, struct seccomp_notif_resp), _IOW('!', 2, __u64)
const NOTIF_RECV: u64 = 0xc050_2100;
const NOTIF_SEND: u64 = 0xc018_2101;
const NOTIF_ID_VALID: u64 = 0x4008_2102;
const _: () = assert!(size_of::<seccomp_notif>() == 80 && size_of::<seccomp_notif_resp>() == 24);

/// `SECCOMP_USER_NOTIF_FLAG_CONTINUE`: run the call in the process that made it.
const FLAG_CONTINUE: u32 = 1;

enum Answer {
    /// Let the kernel run the call. Only for calls whose arguments are numbers: a pointer could
    /// be changed after the check.
    Continue,
    /// The call's result is 0; the supervisor did the work.
    Done,
    Error(i32),
}

/// The file a call names.
enum Target {
    /// An open descriptor of the calling process.
    Fd(i32),
    /// A path at `addr` in the caller's memory, relative to `dirfd`.
    Path { dirfd: i32, addr: u64, follow: bool, empty_ok: bool },
}

enum Change {
    Mode(u32),
    Owner(u32, u32),
    Times(Times),
}

/// Where the caller's two times are, by the layout of the call that passed them.
enum Times {
    Timespec(u64),
    Timeval(u64),
    Utimbuf(u64),
}

/// The processes of one command, and the listener that answers for them.
pub(crate) struct Tracker {
    listener: OwnedFd,
    root: i32,
    /// The sample's folder (its real path): the one place where modes, owners and times change.
    folder: Vec<u8>,
    /// Session numbers the command's processes are in: the shell's, and every `setsid` since.
    sessions: Mutex<HashSet<i32>>,
    /// Stellarator runs as root: `chown` then needs the limits an unprivileged caller has.
    privileged: bool,
    empty: AtomicBool,
    /// Held while a thread reads the listener.
    reader: Mutex<()>,
}

impl Tracker {
    pub(crate) fn new(listener: OwnedFd, root: i32, folder: &std::path::Path) -> Self {
        use std::os::unix::ffi::OsStrExt as _;
        Self {
            listener,
            root,
            folder: folder.as_os_str().as_bytes().to_vec(),
            sessions: Mutex::new(HashSet::from([root])),
            // SAFETY: geteuid cannot fail.
            privileged: unsafe { libc::geteuid() } == 0,
            empty: AtomicBool::new(false),
            reader: Mutex::new(()),
        }
    }

    /// Answers the calls that are waiting, for up to `wait`. Returns early once the command has no
    /// process left.
    pub(crate) fn service(&self, wait: Duration) {
        // One reader at a time: a second thread that saw the same call waiting would block in the
        // receive until the next one came.
        let Ok(_reader) = self.reader.try_lock() else {
            std::thread::sleep(wait.min(Duration::from_millis(5)));
            return;
        };
        let mut timeout = wait.as_millis().min(i32::MAX as u128) as i32;
        loop {
            let mut poll = libc::pollfd { fd: self.listener.as_raw_fd(), events: libc::POLLIN, revents: 0 };
            // SAFETY: one valid pollfd.
            let ready = unsafe { libc::poll(&mut poll, 1, timeout) };
            if ready <= 0 {
                return;
            }
            if poll.revents & libc::POLLIN != 0 {
                if let Some(request) = self.receive() {
                    let answer = self.decide(&request);
                    self.respond(request.id, answer);
                }
                // More may be waiting; do not sleep again.
                timeout = 0;
                continue;
            }
            if poll.revents & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL) != 0 {
                self.empty.store(true, Ordering::Release);
            }
            return;
        }
    }

    /// Whether every process that ran under the command's filter is gone, as the kernel reports
    /// it on the listener.
    pub(crate) fn is_empty(&self) -> bool {
        if self.empty.load(Ordering::Acquire) {
            return true;
        }
        let mut poll = libc::pollfd { fd: self.listener.as_raw_fd(), events: 0, revents: 0 };
        // SAFETY: one valid pollfd; a zero timeout does not block.
        let ready = unsafe { libc::poll(&mut poll, 1, 0) };
        let gone = ready > 0 && poll.revents & libc::POLLHUP != 0;
        if gone {
            self.empty.store(true, Ordering::Release);
        }
        gone
    }

    /// Kills every process of the command. `root_running`: the shell has not been waited for yet,
    /// so its process group number is still its own. Returns whether the tree is known to be gone.
    pub(crate) fn kill(&self, root_running: bool) -> bool {
        if root_running {
            // SAFETY: the group is the shell's (it called setsid), and the shell is unreaped, so
            // the number cannot have been reused.
            unsafe { libc::kill(-self.root, libc::SIGKILL) };
        }
        let mut quiet = 0;
        for round in 0..300u64 {
            if self.is_empty() {
                return true;
            }
            let sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner()).clone();
            let mut alive = 0;
            for pid in procfs::pids() {
                let Some(stat) = procfs::stat(pid) else { continue };
                if matches!(stat.state, b'Z' | b'X') || !sessions.contains(&stat.session) {
                    continue;
                }
                if !procfs::is_sandboxed(pid) {
                    continue;
                }
                alive += 1;
                // SAFETY: a signal to one process, found in the command's session a moment ago.
                unsafe { libc::kill(pid, libc::SIGKILL) };
            }
            // Processes the kernel has not released yet (zombies nobody waits for) keep the
            // listener open; three scans without a live one is as sure as it gets.
            quiet = if alive == 0 { quiet + 1 } else { 0 };
            if quiet >= 3 {
                return true;
            }
            // Keep answering: a process stopped in one of our calls is released by its kill, and
            // a session started meanwhile is recorded.
            self.service(Duration::from_millis((1 + round).min(10)));
        }
        self.is_empty()
    }

    fn receive(&self) -> Option<seccomp_notif> {
        // SAFETY: the kernel wants the struct zeroed and fills it.
        let mut request: seccomp_notif = unsafe { std::mem::zeroed() };
        // SAFETY: a valid listener and a struct of the size the request code names.
        let result = unsafe { libc::ioctl(self.listener.as_raw_fd(), NOTIF_RECV as _, &mut request) };
        (result == 0).then_some(request)
    }

    fn still_waiting(&self, id: u64) -> bool {
        // SAFETY: a valid listener and a u64.
        unsafe { libc::ioctl(self.listener.as_raw_fd(), NOTIF_ID_VALID as _, &id) == 0 }
    }

    fn respond(&self, id: u64, answer: Answer) {
        let (error, flags) = match answer {
            Answer::Continue => (0, FLAG_CONTINUE),
            Answer::Done => (0, 0),
            Answer::Error(code) => (-code, 0),
        };
        let response = seccomp_notif_resp { id, val: 0, error, flags };
        // The caller may have been killed meanwhile; then there is nobody to answer.
        // SAFETY: a valid listener and a response of the size the request code names.
        unsafe { libc::ioctl(self.listener.as_raw_fd(), NOTIF_SEND as _, &response) };
    }

    fn decide(&self, request: &seccomp_notif) -> Answer {
        let nr = request.data.nr as c_long;
        let args = request.data.args;
        let tid = request.pid as i32;
        if nr == libc::SYS_setsid {
            // The session's number will be the caller's process number.
            if let Some(process) = procfs::tgid(tid) {
                self.sessions.lock().unwrap_or_else(|e| e.into_inner()).insert(process);
            }
            return Answer::Continue;
        }
        if seccomp::signal_calls().contains(&nr) {
            return if self.may_signal(nr, tid, &args) { Answer::Continue } else { Answer::Error(libc::EPERM) };
        }
        match metadata_call(nr, &args) {
            Some((target, change)) => match self.change(request.id, tid, target, change) {
                Ok(()) => Answer::Done,
                Err(code) => Answer::Error(code),
            },
            None => Answer::Error(libc::ENOSYS),
        }
    }

    // ---- Signals ------------------------------------------------------------------------------

    fn in_command(&self, pid: i32) -> bool {
        pid > 0
            && procfs::stat(pid)
                .is_some_and(|s| self.sessions.lock().unwrap_or_else(|e| e.into_inner()).contains(&s.session))
    }

    fn group_in_command(&self, pgrp: i32) -> bool {
        let sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner()).clone();
        // The group's leader, when it is still there, answers without a scan.
        if let Some(leader) = procfs::stat(pgrp) {
            if leader.pgrp == pgrp {
                return sessions.contains(&leader.session);
            }
        }
        // A process group lies within one session, so any member answers for all.
        procfs::pids()
            .into_iter()
            .filter_map(procfs::stat)
            .find(|s| s.pgrp == pgrp)
            .is_some_and(|s| sessions.contains(&s.session))
    }

    fn may_signal(&self, nr: c_long, tid: i32, args: &[u64; 6]) -> bool {
        let first = args[0] as i32;
        if nr == libc::SYS_kill {
            return match first {
                0 => true,   // the caller's own group
                -1 => false, // every process of the user
                pid if pid > 0 => self.in_command(pid),
                group => group != i32::MIN && self.group_in_command(-group),
            };
        }
        if nr == libc::SYS_tgkill || nr == libc::SYS_rt_tgsigqueueinfo {
            return self.in_command(first) && self.in_command(args[1] as i32);
        }
        if nr == seccomp::SYS_PIDFD_SEND_SIGNAL {
            return procfs::pidfd_target(tid, first).is_some_and(|pid| self.in_command(pid));
        }
        // tkill, rt_sigqueueinfo
        self.in_command(first)
    }

    // ---- Modes, owners, times -----------------------------------------------------------------

    fn change(&self, id: u64, tid: i32, target: Target, change: Change) -> Result<(), i32> {
        let times = match &change {
            Change::Times(times) => load_times(tid, times)?,
            _ => None,
        };
        let file = open_target(tid, target)?;
        // The memory and the descriptors read above belong to the caller only while it waits.
        if !self.still_waiting(id) {
            return Err(libc::ESRCH);
        }
        if !self.inside_folder(&file) {
            return Err(libc::EPERM);
        }
        // "/proc/self/fd/N" names the opened file itself, whatever happens to its path now.
        let name = CString::new(format!("/proc/self/fd/{}", file.as_raw_fd())).map_err(|_| libc::EINVAL)?;
        // SAFETY (all three): a valid path; for times, a null pointer or two timespecs.
        let result = match change {
            Change::Mode(mode) => unsafe { libc::fchmodat(libc::AT_FDCWD, name.as_ptr(), mode & 0o7777, 0) },
            Change::Owner(uid, gid) => {
                if self.privileged && !same_owner(&file, uid, gid) {
                    // Stellarator is root and could give the file to anyone; the command has no
                    // such right.
                    return Err(libc::EPERM);
                }
                unsafe { libc::fchownat(libc::AT_FDCWD, name.as_ptr(), uid, gid, 0) }
            }
            Change::Times(_) => {
                let pointer = times.as_ref().map_or(std::ptr::null(), |t| t.as_ptr());
                unsafe { libc::utimensat(libc::AT_FDCWD, name.as_ptr(), pointer, 0) }
            }
        };
        if result == 0 {
            Ok(())
        } else {
            Err(last_errno())
        }
    }

    /// Whether the opened file is in the sample's folder, or is no file on disk at all (a pipe, a
    /// socket pair).
    fn inside_folder(&self, file: &OwnedFd) -> bool {
        let Some(location) = location(file.as_raw_fd()) else { return false };
        if [&b"pipe:["[..], b"socket:[", b"anon_inode:"].iter().any(|p| location.starts_with(p)) {
            return true;
        }
        location == self.folder
            || (location.starts_with(&self.folder) && location.get(self.folder.len()) == Some(&b'/'))
    }
}

fn last_errno() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(libc::EIO)
}

/// Which file a call changes and how, from its number and arguments.
fn metadata_call(nr: c_long, a: &[u64; 6]) -> Option<(Target, Change)> {
    let path = |dirfd: u64, addr: u64, flags: u64| Target::Path {
        dirfd: dirfd as i32,
        addr,
        follow: flags & libc::AT_SYMLINK_NOFOLLOW as u64 == 0,
        empty_ok: flags & libc::AT_EMPTY_PATH as u64 != 0,
    };
    let cwd = libc::AT_FDCWD as u64;
    #[cfg(target_arch = "x86_64")]
    {
        let nofollow = libc::AT_SYMLINK_NOFOLLOW as u64;
        match nr {
            libc::SYS_chmod => return Some((path(cwd, a[0], 0), Change::Mode(a[1] as u32))),
            libc::SYS_chown => return Some((path(cwd, a[0], 0), Change::Owner(a[1] as u32, a[2] as u32))),
            libc::SYS_lchown => return Some((path(cwd, a[0], nofollow), Change::Owner(a[1] as u32, a[2] as u32))),
            libc::SYS_utime => return Some((path(cwd, a[0], 0), Change::Times(Times::Utimbuf(a[1])))),
            libc::SYS_utimes => return Some((path(cwd, a[0], 0), Change::Times(Times::Timeval(a[1])))),
            libc::SYS_futimesat => {
                // Without a path, the descriptor itself.
                let target = if a[1] == 0 { Target::Fd(a[0] as i32) } else { path(a[0], a[1], 0) };
                return Some((target, Change::Times(Times::Timeval(a[2]))));
            }
            _ => {}
        }
    }
    Some(match nr {
        libc::SYS_fchmod => (Target::Fd(a[0] as i32), Change::Mode(a[1] as u32)),
        // The kernel's fchmodat has no flags; fchmodat2 has.
        libc::SYS_fchmodat => (path(a[0], a[1], 0), Change::Mode(a[2] as u32)),
        seccomp::SYS_FCHMODAT2 => (path(a[0], a[1], a[3]), Change::Mode(a[2] as u32)),
        libc::SYS_fchown => (Target::Fd(a[0] as i32), Change::Owner(a[1] as u32, a[2] as u32)),
        libc::SYS_fchownat => (path(a[0], a[1], a[4]), Change::Owner(a[2] as u32, a[3] as u32)),
        libc::SYS_utimensat => {
            // futimens(fd) is utimensat(fd, NULL).
            let target = if a[1] == 0 { Target::Fd(a[0] as i32) } else { path(a[0], a[1], a[3]) };
            (target, Change::Times(Times::Timespec(a[2])))
        }
        _ => return None,
    })
}

/// Copies from the caller's memory. Needs the same right as attaching a debugger to a child, which
/// `kernel.yama.ptrace_scope` 2 and 3 take away; the call then fails with "not permitted".
fn read_memory(tid: i32, addr: u64, buffer: &mut [u8]) -> Result<usize, i32> {
    let local = libc::iovec { iov_base: buffer.as_mut_ptr().cast(), iov_len: buffer.len() };
    let remote = libc::iovec { iov_base: addr as *mut libc::c_void, iov_len: buffer.len() };
    // SAFETY: `local` describes a buffer we own; `remote` is only an address in the other process.
    let read = unsafe { libc::process_vm_readv(tid, &local, 1, &remote, 1, 0) };
    if read < 0 {
        return Err(match last_errno() {
            libc::ESRCH => libc::ESRCH,
            libc::EPERM => libc::EPERM,
            _ => libc::EFAULT,
        });
    }
    Ok(read as usize)
}

fn read_exact(tid: i32, addr: u64, buffer: &mut [u8]) -> Result<(), i32> {
    if read_memory(tid, addr, buffer)? == buffer.len() {
        Ok(())
    } else {
        Err(libc::EFAULT)
    }
}

/// A NUL-terminated path from the caller's memory, without the NUL.
fn read_path(tid: i32, addr: u64) -> Result<Vec<u8>, i32> {
    const PAGE: u64 = 4096;
    let mut path = Vec::new();
    let mut at = addr;
    while path.len() < libc::PATH_MAX as usize {
        // Up to the end of the page: the next one may not be mapped.
        let mut chunk = [0u8; PAGE as usize];
        let wanted = (PAGE - at % PAGE) as usize;
        let read = read_memory(tid, at, &mut chunk[..wanted])?;
        if read == 0 {
            return Err(libc::EFAULT);
        }
        if let Some(end) = chunk[..read].iter().position(|b| *b == 0) {
            path.extend_from_slice(&chunk[..end]);
            return Ok(path);
        }
        path.extend_from_slice(&chunk[..read]);
        at += read as u64;
    }
    Err(libc::ENAMETOOLONG)
}

fn read_longs<const N: usize>(tid: i32, addr: u64) -> Result<[i64; N], i32> {
    let mut bytes = vec![0u8; N * 8];
    read_exact(tid, addr, &mut bytes)?;
    let mut longs = [0i64; N];
    for (long, chunk) in longs.iter_mut().zip(bytes.as_chunks::<8>().0) {
        *long = i64::from_ne_bytes(*chunk);
    }
    Ok(longs)
}

/// The two times of the call, or `None` for "now" (a null pointer).
fn load_times(tid: i32, times: &Times) -> Result<Option<[libc::timespec; 2]>, i32> {
    let spec = |sec: i64, nsec: i64| libc::timespec { tv_sec: sec, tv_nsec: nsec };
    Ok(match *times {
        Times::Timespec(0) | Times::Timeval(0) | Times::Utimbuf(0) => None,
        Times::Timespec(addr) => {
            let [s0, n0, s1, n1] = read_longs::<4>(tid, addr)?;
            Some([spec(s0, n0), spec(s1, n1)])
        }
        Times::Timeval(addr) => {
            let [s0, u0, s1, u1] = read_longs::<4>(tid, addr)?;
            if !(0..1_000_000).contains(&u0) || !(0..1_000_000).contains(&u1) {
                return Err(libc::EINVAL);
            }
            Some([spec(s0, u0 * 1000), spec(s1, u1 * 1000)])
        }
        Times::Utimbuf(addr) => {
            let [access, modification] = read_longs::<2>(tid, addr)?;
            Some([spec(access, 0), spec(modification, 0)])
        }
    })
}

fn open_path(dir: RawFd, path: &[u8], flags: libc::c_int) -> Result<OwnedFd, i32> {
    let c_path = CString::new(path).map_err(|_| libc::EINVAL)?;
    // SAFETY: a valid NUL-terminated path; O_PATH opens nothing for reading or writing.
    let fd = unsafe { libc::openat(dir, c_path.as_ptr(), libc::O_PATH | libc::O_CLOEXEC | flags) };
    if fd < 0 {
        return Err(last_errno());
    }
    // SAFETY: a descriptor we just opened.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

/// Opens, as Stellarator, the file a call of thread `tid` names: a handle on the file itself that
/// can be checked and then changed, wherever its path points afterwards.
fn open_target(tid: i32, target: Target) -> Result<OwnedFd, i32> {
    // The caller's working directory or one of its descriptors, through /proc.
    let base = |dirfd: i32| {
        if dirfd == libc::AT_FDCWD {
            Ok(format!("/proc/{tid}/cwd"))
        } else if dirfd >= 0 {
            Ok(format!("/proc/{tid}/fd/{dirfd}"))
        } else {
            Err(libc::EBADF)
        }
    };
    let bad_descriptor = |code| if code == libc::ENOENT { libc::EBADF } else { code };
    match target {
        Target::Fd(fd) => open_path(libc::AT_FDCWD, base(fd.max(-1))?.as_bytes(), 0).map_err(bad_descriptor),
        Target::Path { dirfd, addr, follow, empty_ok } => {
            let path = read_path(tid, addr)?;
            let nofollow = if follow { 0 } else { libc::O_NOFOLLOW };
            if path.is_empty() {
                if !empty_ok {
                    return Err(libc::ENOENT);
                }
                return open_path(libc::AT_FDCWD, base(dirfd)?.as_bytes(), 0).map_err(bad_descriptor);
            }
            if path[0] == b'/' {
                return open_path(libc::AT_FDCWD, &path, nofollow);
            }
            let directory = open_path(libc::AT_FDCWD, base(dirfd)?.as_bytes(), libc::O_DIRECTORY).map_err(bad_descriptor)?;
            open_path(directory.as_raw_fd(), &path, nofollow)
        }
    }
}

/// Where an open descriptor's file is, as the kernel names it.
fn location(fd: RawFd) -> Option<Vec<u8>> {
    let name = CString::new(format!("/proc/self/fd/{fd}")).ok()?;
    let mut buffer = vec![0u8; libc::PATH_MAX as usize + 64];
    // SAFETY: a valid path and a buffer of the length given.
    let length = unsafe { libc::readlink(name.as_ptr(), buffer.as_mut_ptr().cast(), buffer.len()) };
    if length <= 0 || length as usize >= buffer.len() {
        return None;
    }
    buffer.truncate(length as usize);
    Some(buffer)
}

/// Whether `chown(uid, gid)` would leave the file's owner and group as they are.
fn same_owner(file: &OwnedFd, uid: u32, gid: u32) -> bool {
    // SAFETY: fstat on a descriptor we own, into a zeroed stat. It works on O_PATH descriptors.
    let stat = unsafe {
        let mut stat: libc::stat = std::mem::zeroed();
        if libc::fstat(file.as_raw_fd(), &mut stat) != 0 {
            return false;
        }
        stat
    };
    (uid == u32::MAX || uid == stat.st_uid) && (gid == u32::MAX || gid == stat.st_gid)
}
