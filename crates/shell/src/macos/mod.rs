//! The macOS sandbox: Seatbelt, through `/usr/bin/sandbox-exec` with a profile made per command.
//!
//! `sandbox-exec` puts the profile on itself and starts the shell, and every process the shell
//! starts inherits it; nothing can take it off. Apple marks the tool deprecated, and it is what
//! Claude Code's and Codex CLI's sandboxes use: there is no other way to confine a child process
//! without an entitlement.

use std::ffi::OsString;
use std::io;
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

use crate::error::Error;
use crate::run::{LimitHit, Limits, Spec};
use crate::support::{Enforced, Mechanism, Support};
use crate::unix::{self, Rlimits};

/// Only this one: a `sandbox-exec` found on `PATH` could be anything.
const SANDBOX_EXEC: &str = "/usr/bin/sandbox-exec";

const PROFILE: &str = include_str!("profile.sbpl");

/// Added when the caller turns the network on: IPv4 and IPv6, name lookups, and the services TLS
/// libraries ask about certificates. Unix sockets stay closed except the resolver's.
const NETWORK: &str = r#"
; ---- The network (the caller turned it on) --------------------------------------------------------
(allow network-outbound (remote ip "*:*"))
(allow network-inbound (local ip "*:*"))
(allow network-bind (local ip "*:*"))
(allow network-outbound (literal "/private/var/run/mDNSResponder"))
(allow system-socket
  (require-all
    (socket-domain AF_SYSTEM)
    (socket-protocol 2)))
(allow mach-lookup
  (global-name "com.apple.SystemConfiguration.DNSConfiguration")
  (global-name "com.apple.SystemConfiguration.configd")
  (global-name "com.apple.dnssd.service")
  (global-name "com.apple.networkd")
  (global-name "com.apple.SecurityServer")
  (global-name "com.apple.ocspd")
  (global-name "com.apple.trustd")
  (global-name "com.apple.trustd.agent")
  (global-name "com.apple.TrustEvaluationAgent"))
(allow sysctl-read (sysctl-name-prefix "net.routetable."))
(allow file-read* (literal "/Library/Preferences/com.apple.networkd.plist"))
"#;

/// Refuses `setsid`, so that no process leaves the command's session and the kill at its end
/// reaches all of them. The rule uses a part of the profile language that not every macOS release
/// has; [`probe`] finds out.
const NO_NEW_SESSION: &str = r#"
; ---- No process leaves the command's session ------------------------------------------------------
(deny syscall-unix (syscall-number SYS_setsid))
"#;

/// Whether this macOS accepts [`NO_NEW_SESSION`].
static SESSIONS_HELD: AtomicBool = AtomicBool::new(false);

/// Where the developer tools are: the folder `xcode-select` points at (Xcode, under whatever name
/// it is installed, or the Command Line Tools). `/usr/bin/git`, `python3` and `cc` are stubs that
/// run the programs in it. For Xcode it is the whole application: its tools load frameworks from
/// beside the `Developer` folder.
fn developer_folders() -> &'static [PathBuf] {
    static FOLDERS: OnceLock<Vec<PathBuf>> = OnceLock::new();
    FOLDERS.get_or_init(|| {
        let mut folders = Vec::new();
        for candidate in ["/var/db/xcode_select_link", "/Applications/Xcode.app/Contents/Developer"] {
            let Ok(real) = std::fs::canonicalize(candidate) else { continue };
            let application = real.ancestors().find(|p| p.extension().is_some_and(|e| e == "app"));
            let folder = application.unwrap_or(&real).to_path_buf();
            if folder.is_dir() && !folders.contains(&folder) {
                folders.push(folder);
            }
        }
        folders
    })
}

/// The profile for one command and the `-D` definitions it refers to.
fn profile(spec: &Spec<'_>, hold_sessions: bool) -> Result<(String, Vec<OsString>), Error> {
    let mut text = String::from(PROFILE);
    let mut definitions = Vec::new();
    let mut define = |key: &str, path: &Path| -> Result<(), Error> {
        let Some(value) = path.to_str() else {
            return Err(Error::InvalidOption(format!("{} is not UTF-8", path.display())));
        };
        definitions.push(OsString::from(format!("-D{key}={value}")));
        Ok(())
    };
    define("FOLDER", spec.folder)?;
    let extra = developer_folders().iter().map(PathBuf::as_path).chain(spec.options.read_paths.iter().map(PathBuf::as_path));
    for (index, path) in extra.enumerate() {
        // The real path: Seatbelt compares against what a path resolves to.
        let real = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let key = format!("READ_{index}");
        define(&key, &real)?;
        text.push_str(&format!(
            "\n(allow file-read* file-test-existence (subpath (param \"{key}\")))\n\
             (allow file-read-metadata file-test-existence (path-ancestors (param \"{key}\")))\n"
        ));
    }
    if spec.options.network {
        text.push_str(NETWORK);
    }
    if hold_sessions {
        text.push_str(NO_NEW_SESSION);
    }
    if let Some(extra) = &spec.options.seatbelt_extra {
        text.push_str(extra);
    }
    Ok((text, definitions))
}

/// Nothing to hold between making the command and starting it.
pub(crate) struct Prepared;

/// `sandbox-exec -p <profile> -D... -- <shell> -c <command>`, with the session and the resource
/// limits set in the child before it starts.
pub(crate) fn prepare(spec: &Spec<'_>) -> Result<(Command, Prepared), Error> {
    prepare_with(spec, SESSIONS_HELD.load(Ordering::Relaxed))
}

fn prepare_with(spec: &Spec<'_>, hold_sessions: bool) -> Result<(Command, Prepared), Error> {
    let (profile, definitions) = profile(spec, hold_sessions)?;
    // SAFETY: geteuid cannot fail.
    let uid = unsafe { libc::geteuid() };
    // The kernel's process limit counts every process of the user, so it cannot be the command's
    // limit: the commands of other samples would use it up. It is set far above, as the bound on
    // what a command can start in the half second between two counts of its own processes.
    let backstop =
        spec.options.limits.processes.map(|limit| user_processes(uid) + limit.saturating_mul(4).max(1024));
    let limits = Rlimits::new(&spec.options.limits, backstop);

    let mut command = Command::new(SANDBOX_EXEC);
    command.arg("-p").arg(profile).args(definitions).arg("--").arg(unix::shell()).arg("-c").arg(spec.command);
    command.current_dir(spec.cwd).env_clear().envs(spec.env.iter().map(|(k, v)| (k, v)));
    // SAFETY: system calls only, on values copied into the closure (see unix.rs).
    unsafe {
        command.pre_exec(move || {
            unix::detach().map_err(io::Error::from_raw_os_error)?;
            unix::close_inherited();
            limits.apply().map_err(io::Error::from_raw_os_error)
        })
    };
    Ok((command, Prepared))
}

impl Prepared {
    pub(crate) fn started(self, pid: u32, _folder: &Path) -> Result<Tracker, Error> {
        Ok(Tracker { root: pid as i32 })
    }

    pub(crate) fn spawn_failed(self, error: io::Error) -> Error {
        Error::io(format!("Starting {SANDBOX_EXEC}"), error)
    }
}

/// Kills a child that started without a tracker.
pub(crate) fn kill_group(pid: u32) {
    if pid > 0 {
        // SAFETY: the child called setsid, so its group is its own, and it is unreaped.
        unsafe { libc::kill(-(pid as i32), libc::SIGKILL) };
    }
}

// libproc, part of libSystem.
const PROC_UID_ONLY: u32 = 4;
extern "C" {
    fn proc_listpids(kind: u32, info: u32, buffer: *mut libc::c_void, size: libc::c_int) -> libc::c_int;
}

/// How many processes `uid` runs: what the kernel compares `RLIMIT_NPROC` with.
fn user_processes(uid: u32) -> u64 {
    // SAFETY: a null buffer asks for a size that fits every process; then a buffer of that size,
    // which the kernel fills with the user's processes and returns the bytes written.
    unsafe {
        let room = proc_listpids(PROC_UID_ONLY, uid, std::ptr::null_mut(), 0);
        if room <= 0 {
            return 0;
        }
        let mut pids = vec![0 as libc::pid_t; room as usize / size_of::<libc::pid_t>() + 64];
        let bytes = (pids.len() * size_of::<libc::pid_t>()) as libc::c_int;
        let filled = proc_listpids(PROC_UID_ONLY, uid, pids.as_mut_ptr().cast(), bytes);
        (filled.max(0) as u64) / size_of::<libc::pid_t>() as u64
    }
}

/// Every process on the machine.
fn all_pids() -> Vec<libc::pid_t> {
    // SAFETY: a null buffer asks for the count; then a buffer of the size given.
    unsafe {
        let count = libc::proc_listallpids(std::ptr::null_mut(), 0);
        if count <= 0 {
            return Vec::new();
        }
        let mut pids = vec![0 as libc::pid_t; count as usize + 64];
        let bytes = (pids.len() * size_of::<libc::pid_t>()) as libc::c_int;
        let filled = libc::proc_listallpids(pids.as_mut_ptr().cast(), bytes);
        pids.truncate(filled.max(0) as usize);
        pids
    }
}

/// The processes of one command: those in the session its shell started.
pub(crate) struct Tracker {
    root: i32,
}

impl Tracker {
    fn members(&self) -> Vec<libc::pid_t> {
        // SAFETY: getsid reads one process's session number.
        all_pids().into_iter().filter(|pid| *pid > 0 && unsafe { libc::getsid(*pid) } == self.root).collect()
    }

    /// Nothing to answer on macOS: Seatbelt decides in the kernel.
    pub(crate) fn service(&self, wait: Duration) {
        std::thread::sleep(wait);
    }

    /// There is no cheap signal for "all gone" on macOS.
    pub(crate) fn hung_up(&self) -> bool {
        false
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.members().is_empty()
    }

    /// Kills every process in the command's session. Returns whether none is left.
    pub(crate) fn kill(&self, root_running: bool) -> bool {
        if root_running {
            // SAFETY: the group is the shell's (it called setsid), and the shell is unreaped.
            unsafe { libc::kill(-self.root, libc::SIGKILL) };
        }
        for round in 0..200u64 {
            let members = self.members();
            // The shell itself stays a member until it is waited for.
            let others: Vec<_> = members.into_iter().filter(|pid| !(root_running && *pid == self.root)).collect();
            if others.is_empty() {
                return true;
            }
            for pid in others {
                // SAFETY: a signal to one process, found in the command's session a moment ago.
                unsafe { libc::kill(pid, libc::SIGKILL) };
            }
            std::thread::sleep(Duration::from_millis((1 + round).min(10)));
        }
        false
    }

    /// The limits the kernel does not keep on macOS, checked from outside: the number of the
    /// command's processes, and the memory of each.
    pub(crate) fn over_limit(&self, limits: &Limits) -> Option<LimitHit> {
        let members = self.members();
        if limits.processes.is_some_and(|limit| members.len() as u64 > limit) {
            return Some(LimitHit::Processes);
        }
        let limit = limits.memory_bytes?;
        let over = members.into_iter().any(|pid| {
            // SAFETY: the kernel fills a zeroed rusage_info_v2 for flavor 2.
            let footprint = unsafe {
                let mut usage: libc::rusage_info_v2 = std::mem::zeroed();
                let buffer = (&mut usage as *mut libc::rusage_info_v2).cast();
                if libc::proc_pid_rusage(pid, libc::RUSAGE_INFO_V2, buffer) != 0 {
                    return false;
                }
                usage.ri_phys_footprint
            };
            footprint > limit
        });
        over.then_some(LimitHit::Memory)
    }
}

/// Whether `sandbox-exec` accepts a profile, by running `/usr/bin/true` under it.
fn accepts(extra: &str) -> Result<(), String> {
    let output = Command::new(SANDBOX_EXEC)
        .arg("-p")
        .arg(format!("{PROFILE}{NETWORK}{extra}"))
        .arg("-DFOLDER=/private/var/empty")
        .arg("--")
        .arg("/usr/bin/true")
        .env_clear()
        .output()
        .map_err(|e| format!("{SANDBOX_EXEC} could not be started: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!("{SANDBOX_EXEC} refused the profile: {}", String::from_utf8_lossy(&output.stderr).trim()))
    }
}

/// What this machine has, before the self-test.
pub(crate) fn probe() -> Support {
    if !Path::new(SANDBOX_EXEC).is_file() {
        return Support::none(format!("{SANDBOX_EXEC} is not on this Mac"));
    }
    if let Err(reason) = accepts("") {
        return Support::none(reason);
    }
    let sessions_held = accepts(NO_NEW_SESSION).is_ok();
    SESSIONS_HELD.store(sessions_held, Ordering::Relaxed);
    let mut notes = vec![
        "Apple marks sandbox-exec deprecated; a macOS release without it would leave Stellarator without a sandbox.".to_string(),
        "Memory is checked twice a second, not held by the kernel: a process can pass the limit for up to half a second.".to_string(),
        "The number of processes is counted twice a second, not held by the kernel: a command can pass the limit for up to half a second.".to_string(),
        "Unix sockets are not available to commands, and with the network off no sockets at all.".to_string(),
    ];
    if !sessions_held {
        notes.push(
            "This macOS does not let the profile refuse setsid: a process that starts its own session is not killed with its command."
                .to_string(),
        );
    }
    Support {
        available: true,
        mechanism: Mechanism::Seatbelt,
        reason: None,
        enforces: Enforced {
            writes_confined: true,
            reads_confined: true,
            network_blocked: true,
            signals_confined: true,
            process_tree_killed: sessions_held,
            cpu_time: true,
            memory: true,
            processes: true,
            file_size: true,
            open_files: true,
        },
        notes,
    }
}
