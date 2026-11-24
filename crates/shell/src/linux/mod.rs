//! The Linux sandbox: Landlock for which files a command may open, seccomp for the network and
//! the other ways out, and a supervisor for what neither can decide alone. All three are put on
//! the child between `fork` and `exec`, so nothing needs to be installed.

mod landlock;
mod notify;
mod procfs;
mod seccomp;

use std::io;
use std::os::fd::{AsRawFd as _, FromRawFd as _, OwnedFd, RawFd};
use std::os::unix::process::CommandExt as _;
use std::path::Path;
use std::process::Command;

use crate::error::Error;
use crate::run::Spec;
use crate::support::{Enforced, Mechanism, Support};
use crate::unix::{self, Rlimits};

pub(crate) use notify::Tracker;

impl Tracker {
    /// Cheap: the kernel reported that the command's last process is gone.
    pub(crate) fn hung_up(&self) -> bool {
        self.is_empty()
    }

    /// Memory is limited by the kernel on Linux (`RLIMIT_DATA`).
    pub(crate) fn over_memory(&self, _limit: u64) -> bool {
        false
    }
}

/// Folders a command may read and run programs from.
const READ_EXEC: &[&str] = &[
    "/usr", "/bin", "/sbin", "/lib", "/lib32", "/lib64", "/libx32",
    // NixOS and Guix keep every program here.
    "/nix/store", "/run/current-system/sw", "/gnu/store",
];

/// Files and folders a command may read. `/etc` is a list, not the folder: `passwd`, `group`,
/// `hostname` and `machine-id` would put the user's name and the computer's identity into the
/// command's output (`ls -l` then shows owners as numbers).
const READ: &[&str] = &[
    // The loader and the C library.
    "/etc/ld.so.cache", "/etc/ld.so.conf", "/etc/ld.so.conf.d", "/etc/ld.so.preload",
    "/etc/ld-musl-x86_64.path", "/etc/ld-musl-aarch64.path",
    "/etc/nsswitch.conf", "/etc/gai.conf", "/etc/host.conf", "/etc/hosts", "/etc/resolv.conf",
    "/etc/protocols", "/etc/services", "/etc/netconfig",
    "/etc/localtime", "/etc/timezone", "/etc/locale.alias", "/etc/locale.conf", "/etc/default/locale",
    // Which program a name stands for (java, python, editor).
    "/etc/alternatives",
    // Certificates and TLS settings.
    "/etc/ssl", "/etc/ca-certificates", "/etc/ca-certificates.conf", "/etc/pki", "/etc/crypto-policies",
    "/etc/gnutls", "/var/lib/ca-certificates",
    // The distribution's name and version.
    "/etc/os-release", "/etc/lsb-release", "/etc/debian_version", "/etc/alpine-release",
    "/etc/redhat-release", "/etc/fedora-release", "/etc/arch-release",
    // System-wide settings of common tools.
    "/etc/gitconfig", "/etc/gitattributes", "/etc/mime.types", "/etc/mailcap", "/etc/magic", "/etc/terminfo",
    "/etc/inputrc", "/etc/shells", "/etc/pip.conf", "/etc/npmrc", "/etc/python3", "/etc/java", "/etc/maven",
    "/etc/fonts", "/etc/perl", "/etc/krb5.conf", "/etc/xdg",
    // The processors and the memory, as numbers.
    "/proc/cpuinfo", "/proc/meminfo", "/proc/stat", "/proc/uptime", "/proc/loadavg", "/proc/version",
    "/proc/filesystems", "/proc/sys",
    "/sys/devices/system/cpu", "/sys/devices/system/node", "/sys/fs/cgroup",
    "/sys/kernel/mm/transparent_hugepage",
];

/// Device files a command may read and write.
const DEVICES: &[&str] = &["/dev/null", "/dev/zero", "/dev/full", "/dev/random", "/dev/urandom"];

/// The Landlock ABI to use: the kernel's, or the older one a test asks for.
fn landlock_abi(requested: Option<u32>) -> Result<u32, String> {
    let kernel = landlock::kernel_abi().map_err(|e| match e.raw_os_error() {
        Some(libc::ENOSYS) => "this kernel has no Landlock (it needs Linux 6.2 or later, built with Landlock)".to_string(),
        Some(libc::EOPNOTSUPP) => {
            "Landlock is built into this kernel but turned off at boot (add landlock to the lsm= boot option)".to_string()
        }
        _ => format!("Landlock cannot be used: {e}"),
    })?;
    if kernel < landlock::MIN_ABI {
        return Err(format!(
            "this kernel's Landlock is version {kernel}; the sandbox needs version {} (Linux 6.2) or later",
            landlock::MIN_ABI
        ));
    }
    Ok(kernel.min(landlock::MAX_ABI).min(requested.unwrap_or(u32::MAX)).max(landlock::MIN_ABI))
}

fn ruleset(abi: u32, spec: &Spec<'_>) -> io::Result<landlock::Ruleset> {
    let ruleset = landlock::Ruleset::new(abi, !spec.options.network)?;
    for path in READ_EXEC {
        ruleset.allow(Path::new(path), landlock::READ_EXEC)?;
    }
    for path in READ {
        ruleset.allow(Path::new(path), landlock::READ)?;
    }
    if spec.options.show_processes {
        ruleset.allow(Path::new("/proc"), landlock::READ)?;
    }
    for path in DEVICES {
        ruleset.allow(Path::new(path), landlock::DEVICE)?;
    }
    for path in &spec.options.read_paths {
        ruleset.allow(path, landlock::READ_EXEC)?;
    }
    if !ruleset.allow(spec.folder, landlock::workspace(abi))? {
        return Err(io::Error::new(io::ErrorKind::NotFound, "the workspace folder is gone"));
    }
    Ok(ruleset)
}

/// What the child does between `fork` and `exec`, as plain values: see `unix.rs` for why.
struct ChildSetup {
    limits: Rlimits,
    ruleset: RawFd,
    filter: Vec<libc::sock_filter>,
    /// The child's end of the socket the listener goes over.
    socket: RawFd,
    root: bool,
}

/// The steps of the child's setup, for the message when one fails.
const STEPS: [&str; 7] = [
    "starting a session",
    "setting the resource limits",
    "dropping capabilities",
    "setting no_new_privs",
    "enforcing the Landlock rules",
    "installing the seccomp filter",
    "handing the seccomp listener to Stellarator",
];

impl ChildSetup {
    /// # Safety
    /// Runs in the child between `fork` and `exec`: system calls only.
    unsafe fn run(&self) -> io::Result<()> {
        let mut step = 0u8;
        let result = self.steps(&mut step);
        if let Err(code) = result {
            // Tell the parent which step it was; it reads this when the spawn fails.
            libc::write(self.socket, (&step as *const u8).cast(), 1);
            return Err(io::Error::from_raw_os_error(code));
        }
        Ok(())
    }

    unsafe fn steps(&self, step: &mut u8) -> Result<(), i32> {
        // prctl is variadic and reads unsigned longs.
        const ZERO: libc::c_ulong = 0;
        unix::detach()?;
        unix::close_inherited();
        *step = 1;
        self.limits.apply()?;
        *step = 2;
        if self.root {
            // Root keeps no capability: not in the bounding set, so no program can get one back.
            for capability in 0..64 as libc::c_ulong {
                libc::prctl(libc::PR_CAPBSET_DROP, capability, ZERO, ZERO, ZERO);
            }
        }
        *step = 3;
        if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1 as libc::c_ulong, ZERO, ZERO, ZERO) != 0 {
            return Err(unix::errno());
        }
        *step = 4;
        if landlock::restrict_self(self.ruleset) != 0 {
            return Err(unix::errno());
        }
        if self.root {
            *step = 2;
            // _LINUX_CAPABILITY_VERSION_3, this thread, and three empty sets.
            let header: [u32; 2] = [0x2008_0522, 0];
            let data = [0u32; 6];
            if libc::syscall(libc::SYS_capset, header.as_ptr(), data.as_ptr()) != 0 {
                return Err(unix::errno());
            }
        }
        *step = 5;
        let program = libc::sock_fprog { len: self.filter.len() as u16, filter: self.filter.as_ptr().cast_mut() };
        // Every argument as a full register: `syscall` is variadic and reads longs.
        let listener = libc::syscall(
            libc::SYS_seccomp,
            libc::SECCOMP_SET_MODE_FILTER as libc::c_long,
            libc::SECCOMP_FILTER_FLAG_NEW_LISTENER as libc::c_long,
            &program as *const libc::sock_fprog,
        ) as RawFd;
        if listener < 0 {
            return Err(unix::errno());
        }
        *step = 6;
        send_descriptor(self.socket, listener)?;
        // The command must not hold the listener: it could answer its own calls.
        libc::close(listener);
        libc::close(self.socket);
        Ok(())
    }
}

/// Sends `descriptor` over a Unix socket (`SCM_RIGHTS`).
unsafe fn send_descriptor(socket: RawFd, descriptor: RawFd) -> Result<(), i32> {
    let mut byte = 0u8;
    let mut data = libc::iovec { iov_base: (&mut byte as *mut u8).cast(), iov_len: 1 };
    // Room for one descriptor, aligned as a control message header.
    let mut control = [0u64; 4];
    let mut message: libc::msghdr = std::mem::zeroed();
    message.msg_iov = &mut data;
    message.msg_iovlen = 1;
    message.msg_control = control.as_mut_ptr().cast();
    message.msg_controllen = libc::CMSG_SPACE(size_of::<RawFd>() as u32) as _;
    let header = libc::CMSG_FIRSTHDR(&message);
    (*header).cmsg_level = libc::SOL_SOCKET;
    (*header).cmsg_type = libc::SCM_RIGHTS;
    (*header).cmsg_len = libc::CMSG_LEN(size_of::<RawFd>() as u32) as _;
    std::ptr::copy_nonoverlapping((&descriptor as *const RawFd).cast::<u8>(), libc::CMSG_DATA(header), size_of::<RawFd>());
    if libc::sendmsg(socket, &message, libc::MSG_NOSIGNAL) < 0 {
        return Err(unix::errno());
    }
    Ok(())
}

/// Receives the descriptor the child sent.
fn receive_descriptor(socket: &OwnedFd) -> io::Result<OwnedFd> {
    let mut byte = 0u8;
    let mut data = libc::iovec { iov_base: (&mut byte as *mut u8).cast(), iov_len: 1 };
    let mut control = [0u64; 4];
    // SAFETY: a zeroed msghdr is valid; the buffers above outlive the call.
    unsafe {
        let mut message: libc::msghdr = std::mem::zeroed();
        message.msg_iov = &mut data;
        message.msg_iovlen = 1;
        message.msg_control = control.as_mut_ptr().cast();
        message.msg_controllen = size_of_val(&control) as _;
        if libc::recvmsg(socket.as_raw_fd(), &mut message, libc::MSG_CMSG_CLOEXEC) < 0 {
            return Err(io::Error::last_os_error());
        }
        let header = libc::CMSG_FIRSTHDR(&message);
        if header.is_null() || (*header).cmsg_level != libc::SOL_SOCKET || (*header).cmsg_type != libc::SCM_RIGHTS {
            return Err(io::Error::other("the child sent no seccomp listener"));
        }
        let mut descriptor: RawFd = -1;
        std::ptr::copy_nonoverlapping(libc::CMSG_DATA(header), (&mut descriptor as *mut RawFd).cast::<u8>(), size_of::<RawFd>());
        Ok(OwnedFd::from_raw_fd(descriptor))
    }
}

/// What the parent holds between making the command and starting it.
pub(crate) struct Prepared {
    /// Kept open until the child has enforced it.
    _ruleset: landlock::Ruleset,
    parent_socket: OwnedFd,
    child_socket: OwnedFd,
}

/// The shell command for `spec`, with the sandbox applied in the child before it starts.
pub(crate) fn prepare(spec: &Spec<'_>) -> Result<(Command, Prepared), Error> {
    let abi = landlock_abi(spec.options.landlock_abi).map_err(|reason| Error::Unsupported { reason })?;
    let ruleset = ruleset(abi, spec).map_err(|e| Error::Sandbox(format!("the Landlock rules could not be made: {e}")))?;

    let mut sockets = [0 as RawFd; 2];
    // SAFETY: socketpair fills the two-element array.
    if unsafe { libc::socketpair(libc::AF_UNIX, libc::SOCK_STREAM | libc::SOCK_CLOEXEC, 0, sockets.as_mut_ptr()) } != 0 {
        return Err(Error::Sandbox(format!("socketpair: {}", io::Error::last_os_error())));
    }
    // SAFETY: two descriptors we just made.
    let (parent_socket, child_socket) = unsafe { (OwnedFd::from_raw_fd(sockets[0]), OwnedFd::from_raw_fd(sockets[1])) };

    // SAFETY: geteuid cannot fail.
    let uid = unsafe { libc::geteuid() };
    // The kernel does not apply RLIMIT_NPROC to root.
    let processes_now = (uid != 0 && spec.options.limits.processes.is_some()).then(|| procfs::user_tasks(uid));
    let setup = ChildSetup {
        limits: Rlimits::new(&spec.options.limits, processes_now),
        ruleset: ruleset.raw_fd(),
        // Before ABI 6 Landlock does not keep signals inside; the supervisor does.
        filter: seccomp::program(spec.options.network, abi < 6),
        socket: child_socket.as_raw_fd(),
        root: uid == 0,
    };

    let mut command = Command::new(unix::shell());
    command.arg("-c").arg(spec.command).current_dir(spec.cwd).env_clear().envs(spec.env.iter().map(|(k, v)| (k, v)));
    // SAFETY: `ChildSetup::run` makes system calls only, on values that live in the closure.
    unsafe { command.pre_exec(move || setup.run()) };
    Ok((command, Prepared { _ruleset: ruleset, parent_socket, child_socket }))
}

impl Prepared {
    /// The child runs the shell: take the listener it sent and start tracking its processes.
    pub(crate) fn started(self, pid: u32, folder: &Path) -> Result<Tracker, Error> {
        // Only the child's copy stays, so a child that died without sending reads as end-of-file.
        drop(self.child_socket);
        let listener = receive_descriptor(&self.parent_socket)
            .map_err(|e| Error::Sandbox(format!("the seccomp listener did not arrive: {e}")))?;
        Ok(Tracker::new(listener, pid as i32, folder))
    }

    /// Why the child did not start: the step of the sandbox setup that failed, if it was one.
    pub(crate) fn spawn_failed(self, error: io::Error) -> Error {
        drop(self.child_socket);
        let mut step = [0u8; 1];
        // SAFETY: a one-byte read into a one-byte buffer; MSG_DONTWAIT keeps it from blocking.
        let read = unsafe { libc::recv(self.parent_socket.as_raw_fd(), step.as_mut_ptr().cast(), 1, libc::MSG_DONTWAIT) };
        match STEPS.get(step[0] as usize) {
            Some(step) if read == 1 => Error::Sandbox(format!("{step} failed: {error}")),
            _ => Error::io(format!("Starting {}", unix::shell().display()), error),
        }
    }
}

/// Kills a child that started without a tracker.
pub(crate) fn kill_group(pid: u32) {
    if pid > 0 {
        // SAFETY: the child called setsid, so its group is its own, and it is unreaped.
        unsafe { libc::kill(-(pid as i32), libc::SIGKILL) };
    }
}

/// What this machine has, before the self-test.
pub(crate) fn probe() -> Support {
    let unavailable = |mechanism, reason: String| Support {
        available: false,
        mechanism,
        reason: Some(reason),
        enforces: Enforced::default(),
        notes: Vec::new(),
    };
    if !seccomp::ARCH_SUPPORTED {
        return unavailable(Mechanism::None, format!("no seccomp filter is written for {} processors", std::env::consts::ARCH));
    }
    let abi = match landlock_abi(None) {
        Ok(abi) => abi,
        Err(reason) => return unavailable(Mechanism::None, reason),
    };
    // SAFETY: geteuid cannot fail.
    let root = unsafe { libc::geteuid() } == 0;
    let mut notes = vec![
        "Unix sockets are not available to commands, and with the network off no sockets at all.".to_string(),
        "A command can see whether a path exists outside the workspace, and its size and dates, but cannot list or read it.".to_string(),
        "The process limit counts every process of the user, not the command's alone.".to_string(),
    ];
    if abi < 6 {
        notes.push(format!(
            "Landlock version {abi}: signals are checked by Stellarator's supervisor instead of the kernel (Linux 6.12 does it in the kernel)."
        ));
    }
    if root {
        notes.push("Stellarator runs as root: the kernel does not limit the number of processes for root.".to_string());
    }
    Support {
        available: true,
        mechanism: Mechanism::Landlock { abi },
        reason: None,
        enforces: Enforced {
            writes_confined: true,
            reads_confined: true,
            network_blocked: true,
            signals_confined: true,
            process_tree_killed: true,
            cpu_time: true,
            memory: true,
            processes: !root,
            file_size: true,
            open_files: true,
        },
        notes,
    }
}
