//! The macOS sandbox: Seatbelt, through `/usr/bin/sandbox-exec` with a profile made per command.
//!
//! `sandbox-exec` puts the profile on itself and starts the shell, and every process the shell
//! starts inherits it; nothing can take it off. Apple marks the tool deprecated, and it is what
//! Claude Code's and Codex CLI's sandboxes use: there is no other way to confine a child process
//! without an entitlement.
//!
//! Three parts of this module use what not every macOS release may have, so [`probe`] tries each
//! on the machine and the sandbox works without the ones that fail:
//!
//! - the mark ([`Mark`]): how every process of a command is found when the command ends, whatever
//!   session or process group it moved to;
//! - [`NO_NEW_SESSION`]: the fallback when the mark cannot be read, which keeps processes in the
//!   shell's session by refusing `setsid`;
//! - [`NO_SYSTEM_V`]: System V shared memory, which the profile's "refuse everything" does not
//!   cover.

use std::ffi::{CStr, CString, OsString};
use std::io;
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

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

/// Refuses `setsid` ("Operation not permitted"), so that no process leaves the command's session
/// and the kill at its end reaches all of them. Used only where the mark cannot be read. It does
/// not cover a process started in a new session by `posix_spawn`.
const NO_NEW_SESSION: &str = r#"
; ---- No process leaves the command's session ------------------------------------------------------
(deny syscall-unix (syscall-number SYS_setsid))
"#;

/// Refuses the System V shared memory, semaphore and message queue calls. Seatbelt lets them
/// through under a profile that refuses everything else (a sandboxed `shmget` succeeded on macOS
/// 26), and their objects are shared by number with every program of the user.
const NO_SYSTEM_V: &str = r#"
; ---- System V shared memory, semaphores and message queues ----------------------------------------
(deny syscall-unix
  (syscall-number
    SYS_shmsys SYS_shmat SYS_shmctl SYS_shmdt SYS_shmget
    SYS_semsys SYS_semctl SYS_semget SYS_semop
    SYS_msgsys SYS_msgctl SYS_msgget SYS_msgsnd SYS_msgrcv))
"#;

/// What [`probe`] found this macOS to accept.
static MARKS_WORK: AtomicBool = AtomicBool::new(false);
static SESSIONS_HELD: AtomicBool = AtomicBool::new(false);
static SYSTEM_V_CLOSED: AtomicBool = AtomicBool::new(false);

// The sandbox library's query function, part of libSystem. It is not in Apple's public headers;
// Chromium and WebKit call it the same way. `probe` checks that it answers as expected before
// anything relies on it.
extern "C" {
    fn sandbox_check(pid: libc::pid_t, operation: *const libc::c_char, kind: libc::c_int, ...) -> libc::c_int;
}
/// `SANDBOX_FILTER_GLOBAL_NAME | SANDBOX_CHECK_NO_REPORT`: the argument is a Mach service name,
/// and a refusal is not written to the log.
const BY_SERVICE_NAME_QUIETLY: libc::c_int = 2 | 0x4000_0000;

/// A name that only one command's profile allows: the profile's own fingerprint. A process is the
/// command's when it is sandboxed, its sandbox would let it look up `allowed` (a Mach service
/// that does not exist, so the rule gives it nothing), and would not let it look up `refused`.
/// The second test leaves out processes under a profile that allows everything.
struct Mark {
    allowed: CString,
    refused: CString,
}

impl Mark {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos());
        let name = format!(
            "stellarator.sandbox.{}.{}.{nanos}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        Self {
            refused: CString::new(format!("{name}.not")).expect("no NUL in the name"),
            allowed: CString::new(name).expect("no NUL in the name"),
        }
    }

    fn rule(&self) -> String {
        format!(
            "\n; This command's processes are found by this name when it ends.\n\
             (allow mach-lookup (global-name \"{}\"))\n",
            self.allowed.to_string_lossy()
        )
    }

    fn is_on(&self, pid: libc::pid_t) -> bool {
        let may_look_up = |name: &CStr| {
            // SAFETY: valid NUL-terminated strings; the function only reads them.
            unsafe { sandbox_check(pid, c"mach-lookup".as_ptr(), BY_SERVICE_NAME_QUIETLY, name.as_ptr()) == 0 }
        };
        // SAFETY: a null operation asks whether the process is sandboxed at all.
        let sandboxed = unsafe { sandbox_check(pid, std::ptr::null(), 0) } > 0;
        sandboxed && may_look_up(&self.allowed) && !may_look_up(&self.refused)
    }
}

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
fn profile(spec: &Spec<'_>, mark: Option<&Mark>) -> Result<(String, Vec<OsString>), Error> {
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
    if SYSTEM_V_CLOSED.load(Ordering::Relaxed) {
        text.push_str(NO_SYSTEM_V);
    }
    match mark {
        Some(mark) => text.push_str(&mark.rule()),
        None if SESSIONS_HELD.load(Ordering::Relaxed) => text.push_str(NO_NEW_SESSION),
        None => {}
    }
    Ok((text, definitions))
}

/// What the parent holds between making the command and starting it.
pub(crate) struct Prepared {
    mark: Option<Mark>,
}

/// `sandbox-exec -p <profile> -D... -- <shell> -c <command>`, with the session and the resource
/// limits set in the child before it starts.
pub(crate) fn prepare(spec: &Spec<'_>) -> Result<(Command, Prepared), Error> {
    let mark = MARKS_WORK.load(Ordering::Relaxed).then(Mark::new);
    let (profile, definitions) = profile(spec, mark.as_ref())?;
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
    Ok((command, Prepared { mark }))
}

impl Prepared {
    pub(crate) fn started(self, pid: u32, _folder: &Path) -> Result<Tracker, Error> {
        Ok(Tracker { root: pid as i32, mark: self.mark })
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

/// The processes of one command: those under its profile (by the mark), and those in the session
/// its shell started.
pub(crate) struct Tracker {
    root: i32,
    mark: Option<Mark>,
}

impl Tracker {
    fn members(&self) -> Vec<libc::pid_t> {
        let own = std::process::id() as libc::pid_t;
        all_pids()
            .into_iter()
            .filter(|pid| *pid > 0 && *pid != own)
            // SAFETY: getsid reads one process's session number.
            .filter(|pid| unsafe { libc::getsid(*pid) } == self.root || self.mark.as_ref().is_some_and(|m| m.is_on(*pid)))
            .collect()
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

    /// Kills every process of the command. Returns whether none is left.
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
                // SAFETY: a signal to one process, found to be the command's a moment ago.
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

fn sandbox_exec(extra: &str, program: &[&str]) -> Command {
    let mut command = Command::new(SANDBOX_EXEC);
    command.arg("-p").arg(format!("{PROFILE}{NETWORK}{extra}")).arg("-DFOLDER=/private/var/empty").arg("--").args(program);
    command.env_clear().stdin(std::process::Stdio::null());
    command
}

/// Whether `sandbox-exec` accepts the profile with `extra` added, by running `/usr/bin/true` under
/// it.
fn accepts(extra: &str) -> Result<(), String> {
    let output = sandbox_exec(extra, &["/usr/bin/true"])
        .output()
        .map_err(|e| format!("{SANDBOX_EXEC} could not be started: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!("{SANDBOX_EXEC} refused the profile: {}", String::from_utf8_lossy(&output.stderr).trim()))
    }
}

/// Whether a command's processes can be told by their mark on this macOS: a process under a
/// profile with a mark must show it, and must not show another mark; this process, which has no
/// sandbox, must show none.
fn marks_work() -> bool {
    let (mark, other) = (Mark::new(), Mark::new());
    let Ok(mut child) = sandbox_exec(&mark.rule(), &["/bin/sleep", "10"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    else {
        return false;
    };
    let pid = child.id() as libc::pid_t;
    // The profile is on once `sandbox-exec` has become `sleep`.
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut shown = false;
    while !shown && Instant::now() < deadline {
        shown = mark.is_on(pid);
        if !shown {
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    let exact = !other.is_on(pid) && !mark.is_on(std::process::id() as libc::pid_t) && !mark.is_on(1);
    let _ = child.kill();
    let _ = child.wait();
    shown && exact
}

/// What this machine has, before the self-test.
pub(crate) fn probe() -> Support {
    if !Path::new(SANDBOX_EXEC).is_file() {
        return Support::none(format!("{SANDBOX_EXEC} is not on this Mac"));
    }
    if let Err(reason) = accepts("") {
        return Support::none(reason);
    }
    let marks = marks_work();
    let sessions_held = !marks && accepts(NO_NEW_SESSION).is_ok();
    let system_v_closed = accepts(NO_SYSTEM_V).is_ok();
    MARKS_WORK.store(marks, Ordering::Relaxed);
    SESSIONS_HELD.store(sessions_held, Ordering::Relaxed);
    SYSTEM_V_CLOSED.store(system_v_closed, Ordering::Relaxed);

    let mut notes = vec![
        "Apple marks sandbox-exec deprecated; a macOS release without it would leave Stellarator without a sandbox.".to_string(),
        "Memory and the number of processes are checked twice a second, not held by the kernel: a command can pass either limit for up to half a second.".to_string(),
        "Unix sockets are not available to commands, and with the network off no sockets at all.".to_string(),
    ];
    if !marks {
        notes.push(if sessions_held {
            "The processes of a command cannot be told by their sandbox on this macOS, so setsid is refused instead; a process started in a new session by posix_spawn is not killed with its command.".to_string()
        } else {
            "The processes of a command cannot be told by their sandbox on this macOS: a process that starts its own session is not killed with its command.".to_string()
        });
    }
    if !system_v_closed {
        notes.push("System V shared memory is not closed on this macOS: a command can attach a segment of another program of the user.".to_string());
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
            process_tree_killed: marks,
            system_v_closed,
            cpu_time: true,
            memory: true,
            processes: true,
            file_size: true,
            open_files: true,
        },
        notes,
    }
}
