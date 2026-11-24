//! What sandbox this machine has.

use std::sync::OnceLock;

/// The mechanism that confines commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mechanism {
    /// Linux: Landlock (this ABI version), seccomp and Stellarator's supervisor.
    Landlock { abi: u32 },
    /// macOS: Seatbelt, through `/usr/bin/sandbox-exec`.
    Seatbelt,
    /// No sandbox: commands do not run.
    None,
}

/// The rules the mechanism keeps on this machine. `docs/shell-sandbox.md` says how, and what each
/// leaves open.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Enforced {
    /// A command changes nothing outside its sample's folder: no file, mode, owner or date.
    pub writes_confined: bool,
    /// A command reads its sample's folder and the system's programs and libraries, not the
    /// user's files, other samples or Stellarator's data.
    pub reads_confined: bool,
    /// A command has no network unless the caller turns it on.
    pub network_blocked: bool,
    /// A command signals its own processes only.
    pub signals_confined: bool,
    /// When a command ends or times out, every process it started is killed, also one that
    /// started a session of its own.
    pub process_tree_killed: bool,
    /// System V shared memory, semaphores and message queues, which every program of the user
    /// shares by number, are refused.
    pub system_v_closed: bool,
    /// The processor time of a process is limited.
    pub cpu_time: bool,
    /// The memory of a process is limited.
    pub memory: bool,
    /// The number of processes is limited.
    pub processes: bool,
    /// The size of a file a command writes is limited.
    pub file_size: bool,
    /// The number of open files is limited.
    pub open_files: bool,
}

/// What [`support`] found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Support {
    /// Commands can run. When `false`, [`crate::Workspace::run`] returns
    /// [`crate::Error::Unsupported`] with `reason`.
    pub available: bool,
    pub mechanism: Mechanism,
    /// Why there is no sandbox, in a sentence for the user.
    pub reason: Option<String>,
    pub enforces: Enforced,
    /// Limits of the sandbox on this machine that a user should know.
    pub notes: Vec<String>,
}

impl Support {
    #[cfg_attr(any(target_os = "linux", target_os = "macos"), allow(dead_code))]
    pub(crate) fn none(reason: impl Into<String>) -> Self {
        Self {
            available: false,
            mechanism: Mechanism::None,
            reason: Some(reason.into()),
            enforces: Enforced::default(),
            notes: Vec::new(),
        }
    }
}

/// Which sandbox this machine has and what it enforces. The first call checks the kernel (Linux)
/// or `sandbox-exec` (macOS) and runs one command through the sandbox to see that it works here;
/// the answer is kept for the life of the process.
pub fn support() -> &'static Support {
    static SUPPORT: OnceLock<Support> = OnceLock::new();
    SUPPORT.get_or_init(probe)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn probe() -> Support {
    let mut support = crate::platform::probe();
    if !support.available {
        return support;
    }
    if let Err(reason) = self_test() {
        support.available = false;
        support.reason = Some(format!("the sandbox did not pass its self-test on this machine: {reason}"));
        support.enforces = Enforced::default();
    }
    support
}

/// Runs `echo` in a throwaway folder through the same path as every command. A container that
/// blocks seccomp, a kernel that refuses the ruleset or a `sandbox-exec` that rejects the profile
/// fails here, once, with its reason, instead of at every command.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn self_test() -> Result<(), String> {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let name = format!("stellarator-probe-{}-{}", std::process::id(), COUNTER.fetch_add(1, Ordering::Relaxed));
    let folder = std::env::temp_dir().join(name);
    std::fs::create_dir(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    let result = (|| {
        let folder = std::fs::canonicalize(&folder).map_err(|e| e.to_string())?;
        let options = crate::RunOptions::default();
        let spec = crate::run::Spec {
            folder: &folder,
            cwd: &folder,
            command: "echo sandbox-ok",
            env: crate::run::environment(&folder, &folder, &folder, &[]).map_err(|e| e.to_string())?,
            options: &options,
        };
        let done = crate::run::run_blocking(&spec, std::time::Duration::from_secs(20)).map_err(|e| e.to_string())?;
        if done.exit_code != Some(0) || done.stdout != b"sandbox-ok\n" {
            return Err(format!(
                "a sandboxed `echo` exited with {:?} and printed {:?} and {:?}",
                done.exit_code,
                String::from_utf8_lossy(&done.stdout),
                String::from_utf8_lossy(&done.stderr)
            ));
        }
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&folder);
    result
}

#[cfg(windows)]
fn probe() -> Support {
    Support::none(
        "Windows has no sandbox in Stellarator yet. Commands would need a low-privilege AppContainer \
         with a job object, and a bash that runs inside one; neither is in place (docs/shell-sandbox.md).",
    )
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn probe() -> Support {
    Support::none(format!("Stellarator has no sandbox for {}", std::env::consts::OS))
}
