//! Running one command in a workspace: the options, the result, and the part that is the same on
//! every platform (the environment, the output caps, the timeout and the watch on the folder's
//! size). What confines the command is in `linux/` and `macos/`.

use std::path::PathBuf;
use std::time::Duration;

use crate::error::Error;
use crate::workspace::Workspace;

/// Limits the operating system keeps on a command's processes.
///
/// `None` leaves a limit off. Each is per process except `processes`, which the kernel counts per
/// user: see `docs/shell-sandbox.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limits {
    /// Seconds of processor time one process may use; it is ended with SIGXCPU.
    pub cpu_seconds: Option<u64>,
    /// Bytes of memory one process may use. Linux: its data and heap (`RLIMIT_DATA`), so an
    /// allocation past it fails. macOS: its footprint is checked twice a second and the command is
    /// stopped when one process is over.
    pub memory_bytes: Option<u64>,
    /// Processes and threads the command may add to what its user already runs.
    pub processes: Option<u64>,
    /// The largest file a process may write; a write past it fails with "File too large".
    pub file_bytes: Option<u64>,
    /// Open files per process.
    pub open_files: Option<u64>,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            cpu_seconds: Some(240),
            memory_bytes: Some(4 << 30),
            processes: Some(256),
            file_bytes: Some(256 << 20),
            open_files: Some(1024),
        }
    }
}

/// How one command runs.
#[derive(Debug, Clone)]
pub struct RunOptions {
    /// The command and everything it started are killed after this long.
    pub timeout: Duration,
    /// `false` (the default): no network at all, the computer's own addresses included.
    /// `true`: IPv4 and IPv6 as the user has them.
    pub network: bool,
    /// The most bytes kept of stdout, and of stderr. Past it the beginning and the end are kept
    /// and the middle is dropped; the command keeps running.
    pub max_output_bytes: usize,
    /// Variables added to the command's environment, which otherwise holds a fixed short list
    /// (`PATH`, `HOME`, `TMPDIR`, `USER`, `SHELL`, `LANG`, `TERM`, `TZ`, `PWD`) and nothing of
    /// Stellarator's own. `HOME`, `TMPDIR` and `PWD` cannot be set here.
    pub env: Vec<(String, String)>,
    /// More folders or files the command may read and run, besides the system's: a toolchain in
    /// the user's home folder, for example. Their paths are then visible to the command.
    pub read_paths: Vec<PathBuf>,
    pub limits: Limits,
    /// Linux: let the command read all of `/proc`, as `ps`, `top` and the Java runtime expect. It
    /// then sees the names and command lines of every process on the computer (never their
    /// environment or memory). Off by default: only its own numbers (`/proc/cpuinfo`, `meminfo`,
    /// ...) are readable. No effect on macOS, where a command sees its own processes only.
    pub show_processes: bool,
    /// For this crate's tests: use Landlock as a kernel with this ABI would.
    #[doc(hidden)]
    pub landlock_abi: Option<u32>,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(60),
            network: false,
            max_output_bytes: 128 * 1024,
            env: Vec::new(),
            read_paths: Vec::new(),
            limits: Limits::default(),
            show_processes: false,
            landlock_abi: None,
        }
    }
}

/// One output stream of a command, capped.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Captured {
    /// The whole output, or its beginning when it was cut. Bytes that are not UTF-8 are replaced.
    pub text: String,
    /// The end of the output when it was cut; empty otherwise.
    pub tail: String,
    /// Bytes dropped between `text` and `tail`.
    pub omitted_bytes: u64,
    /// Bytes the command wrote.
    pub total_bytes: u64,
}

impl Captured {
    /// Whether the output was over the cap.
    pub fn truncated(&self) -> bool {
        self.omitted_bytes > 0
    }
}

/// A limit that stopped a command, other than the timeout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitHit {
    /// The sample's folder grew past its size cap.
    WorkspaceSize,
    /// macOS: a process was over `memory_bytes`.
    Memory,
}

/// What a command did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    pub stdout: Captured,
    pub stderr: Captured,
    /// The shell's exit code; `None` when a signal ended it (the timeout and the limits do).
    pub exit_code: Option<i32>,
    /// The signal that ended the shell, if one did: 9 after a timeout, 24 for the CPU limit.
    pub signal: Option<i32>,
    /// The command ran past `timeout` and was killed.
    pub timed_out: bool,
    /// The command was killed for this limit.
    pub limit: Option<LimitHit>,
    pub duration: Duration,
    /// Every process of the command is known to be gone. `false` only when the kill could not be
    /// confirmed (docs/shell-sandbox.md, "After a command").
    pub all_processes_ended: bool,
}

impl Workspace {
    /// Runs `command` with bash (sh where there is no bash) in the workspace, inside the sandbox.
    ///
    /// The command's processes are all gone when this returns: what it left in the background is
    /// killed with it. Dropping the returned future kills them too.
    ///
    /// Errors are about the sandbox, not the command: a command that fails returns `Ok` with its
    /// exit code. [`Error::Unsupported`] when this machine has no sandbox ([`crate::support`]).
    /// Call it on a Tokio runtime with I/O and time enabled.
    pub async fn run(&self, command: &str, options: &RunOptions) -> Result<Output, Error> {
        let support = crate::support();
        if !support.available {
            let reason = support.reason.clone().unwrap_or_else(|| "no sandbox on this machine".into());
            return Err(Error::Unsupported { reason });
        }
        if command.contains('\0') {
            return Err(Error::InvalidOption("the command has a NUL character".into()));
        }
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            let output = imp::run(self, command, options).await;
            self.measure();
            output
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            let _ = options;
            Err(Error::Unsupported { reason: "no sandbox on this platform".into() })
        }
    }
}

/// Cuts a buffer's end back to whole UTF-8 characters and converts it.
#[cfg(any(target_os = "linux", target_os = "macos", test))]
fn text_from(mut bytes: Vec<u8>) -> String {
    if let Err(e) = std::str::from_utf8(&bytes) {
        if e.error_len().is_none() {
            bytes.truncate(e.valid_up_to());
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

/// One stream's bytes as they arrive, keeping at most `cap`: everything while it fits, then the
/// first half and the last half.
#[cfg(any(target_os = "linux", target_os = "macos", test))]
pub(crate) struct Capture {
    cap: usize,
    head: Vec<u8>,
    tail: std::collections::VecDeque<u8>,
    total: u64,
}

#[cfg(any(target_os = "linux", target_os = "macos", test))]
impl Capture {
    pub(crate) fn new(cap: usize) -> Self {
        Self { cap, head: Vec::new(), tail: std::collections::VecDeque::new(), total: 0 }
    }

    pub(crate) fn push(&mut self, bytes: &[u8]) {
        let was_whole = self.total <= self.cap as u64;
        self.total += bytes.len() as u64;
        if self.total <= self.cap as u64 {
            self.head.extend_from_slice(bytes);
            return;
        }
        let half = self.cap / 2;
        if was_whole {
            // The first time over: what is held splits into the two halves.
            self.head.extend_from_slice(bytes);
            let rest = self.head.split_off(half.min(self.head.len()));
            self.tail.extend(rest);
        } else {
            self.tail.extend(bytes);
        }
        let extra = self.tail.len().saturating_sub(self.cap - half);
        self.tail.drain(..extra);
    }

    pub(crate) fn finish(self) -> Captured {
        let kept = (self.head.len() + self.tail.len()) as u64;
        let mut tail: Vec<u8> = self.tail.into();
        // The cut may be inside a character: drop the pieces of it.
        let partial = tail.iter().take(3).take_while(|b| (**b & 0xc0) == 0x80).count();
        tail.drain(..partial);
        Captured {
            text: text_from(self.head),
            tail: String::from_utf8_lossy(&tail).into_owned(),
            omitted_bytes: self.total - kept,
            total_bytes: self.total,
        }
    }
}

/// What a platform needs to start one command.
#[cfg_attr(not(any(target_os = "linux", target_os = "macos")), allow(dead_code))]
pub(crate) struct Spec<'a> {
    /// The sample's folder: the one place the command may write.
    pub(crate) folder: &'a std::path::Path,
    pub(crate) cwd: &'a std::path::Path,
    pub(crate) command: &'a str,
    pub(crate) env: Vec<(String, String)>,
    pub(crate) options: &'a RunOptions,
}

/// The command's whole environment: a fixed list, then the caller's variables.
#[cfg(unix)]
pub(crate) fn environment(
    cwd: &std::path::Path,
    home: &std::path::Path,
    tmp: &std::path::Path,
    extra: &[(String, String)],
) -> Result<Vec<(String, String)>, Error> {
    let path = |p: &std::path::Path| {
        p.to_str().map(str::to_owned).ok_or_else(|| Error::InvalidOption(format!("{} is not UTF-8", p.display())))
    };
    #[cfg(target_os = "macos")]
    let (search, lang) =
        ("/opt/homebrew/bin:/opt/homebrew/sbin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin", "en_US.UTF-8");
    #[cfg(not(target_os = "macos"))]
    let (search, lang) = ("/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin", "C.UTF-8");
    let fixed = [("HOME", path(home)?), ("TMPDIR", path(tmp)?), ("PWD", path(cwd)?)];
    let mut env: Vec<(String, String)> = vec![
        ("PATH".into(), search.into()),
        ("USER".into(), "user".into()),
        ("LOGNAME".into(), "user".into()),
        ("SHELL".into(), crate::unix::shell().display().to_string()),
        ("LANG".into(), lang.into()),
        ("TERM".into(), "dumb".into()),
        ("TZ".into(), "UTC".into()),
    ];
    for (name, value) in extra {
        if name.is_empty() || name.contains(['=', '\0']) || value.contains('\0') {
            return Err(Error::InvalidOption(format!("{name:?} is not a usable environment variable")));
        }
        if fixed.iter().any(|(fixed, _)| fixed == name) {
            continue;
        }
        env.retain(|(existing, _)| existing != name);
        env.push((name.clone(), value.clone()));
    }
    env.extend(fixed.into_iter().map(|(name, value)| (name.to_owned(), value)));
    Ok(env)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod imp {
    use std::process::Stdio;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use tokio::io::{AsyncRead, AsyncReadExt as _};

    use super::{Capture, Captured, LimitHit, Output, RunOptions, Spec};
    use crate::error::Error;
    use crate::platform::{self, Tracker};
    use crate::workspace::{tree_size, Workspace};

    /// How often the monitor looks at the folder's size and (macOS) the processes' memory.
    const CHECK_EVERY: Duration = Duration::from_millis(500);
    /// How long the pipes may stay open after the last process is gone.
    const DRAIN: Duration = Duration::from_secs(2);

    /// Kills the command's processes when the run is dropped before it finished.
    struct Running {
        tracker: Arc<Tracker>,
        stop: Arc<AtomicBool>,
        /// The shell has not been waited for: its process group is still its own.
        root_running: bool,
        finished: bool,
    }

    impl Drop for Running {
        fn drop(&mut self) {
            if !self.finished {
                self.tracker.kill(self.root_running);
            }
            self.stop.store(true, Ordering::Release);
        }
    }

    type Shared = Arc<Mutex<Capture>>;

    async fn capture(mut pipe: impl AsyncRead + Unpin, into: Shared) {
        let mut buffer = vec![0u8; 16 * 1024];
        loop {
            match pipe.read(&mut buffer).await {
                Ok(0) | Err(_) => return,
                Ok(read) => into.lock().unwrap_or_else(|e| e.into_inner()).push(&buffer[..read]),
            }
        }
    }

    /// Waits for a stream's end, for `DRAIN` at most, and returns what arrived.
    async fn collect(task: tokio::task::JoinHandle<()>, from: Shared, cap: usize) -> Captured {
        let abort = task.abort_handle();
        if tokio::time::timeout(DRAIN, task).await.is_err() {
            abort.abort();
        }
        let capture = std::mem::replace(&mut *from.lock().unwrap_or_else(|e| e.into_inner()), Capture::new(cap));
        capture.finish()
    }

    /// The command's thread: answers the sandbox's questions (Linux) and watches the limits that
    /// no kernel setting keeps.
    fn monitor(
        tracker: Arc<Tracker>,
        stop: Arc<AtomicBool>,
        folder: std::path::PathBuf,
        max_bytes: u64,
        memory: Option<u64>,
        hit: tokio::sync::oneshot::Sender<LimitHit>,
    ) {
        let mut hit = Some(hit);
        let mut last_size = tree_size(&folder);
        let mut next_check = Instant::now() + CHECK_EVERY;
        while !stop.load(Ordering::Acquire) && !tracker.hung_up() {
            tracker.service(Duration::from_millis(100));
            if hit.is_none() || Instant::now() < next_check {
                continue;
            }
            let began = Instant::now();
            let size = tree_size(&folder);
            // Over the cap and still growing: a command that deletes its way back is left alone.
            let breach = if size > max_bytes && size > last_size {
                Some(LimitHit::WorkspaceSize)
            } else if memory.is_some_and(|limit| tracker.over_memory(limit)) {
                Some(LimitHit::Memory)
            } else {
                None
            };
            last_size = size;
            // A large folder takes a while to measure: at most a twentieth of the time goes there.
            next_check = Instant::now() + CHECK_EVERY.max(began.elapsed() * 20);
            if let (Some(breach), Some(hit)) = (breach, hit.take()) {
                // The run does the killing: it knows whether the shell has been waited for.
                let _ = hit.send(breach);
            }
        }
    }

    pub(super) async fn run(workspace: &Workspace, command: &str, options: &RunOptions) -> Result<Output, Error> {
        let started = Instant::now();
        let spec = Spec {
            folder: &workspace.root,
            cwd: &workspace.dir,
            command,
            env: super::environment(&workspace.dir, &workspace.home, &workspace.tmp, &options.env)?,
            options,
        };
        let (mut command, prepared) = platform::prepare(&spec)?;
        command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut command = tokio::process::Command::from(command);
        command.kill_on_drop(true);
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => return Err(prepared.spawn_failed(error)),
        };
        drop(command);
        let pid = child.id().unwrap_or(0);
        let tracker = match prepared.started(pid, &workspace.root) {
            Ok(tracker) => Arc::new(tracker),
            Err(error) => {
                platform::kill_group(pid);
                let _ = child.wait().await;
                return Err(error);
            }
        };
        let stop = Arc::new(AtomicBool::new(false));
        let mut running =
            Running { tracker: tracker.clone(), stop: stop.clone(), root_running: true, finished: false };

        let (hit_sender, mut hit_receiver) = tokio::sync::oneshot::channel();
        {
            let (tracker, stop, folder) = (tracker.clone(), stop.clone(), workspace.root.clone());
            let (max_bytes, memory) = (workspace.max_bytes, options.limits.memory_bytes);
            std::thread::Builder::new()
                .name("shell-monitor".into())
                .stack_size(256 * 1024)
                .spawn(move || monitor(tracker, stop, folder, max_bytes, memory, hit_sender))
                .map_err(|e| Error::Sandbox(format!("could not start the monitor thread: {e}")))?;
        }
        let cap = options.max_output_bytes;
        let (stdout, stderr) = (Shared::new(Mutex::new(Capture::new(cap))), Shared::new(Mutex::new(Capture::new(cap))));
        let stdout_task = tokio::spawn(capture(child.stdout.take().expect("piped"), stdout.clone()));
        let stderr_task = tokio::spawn(capture(child.stderr.take().expect("piped"), stderr.clone()));

        let mut timed_out = false;
        let mut limit = None;
        let mut status = None;
        tokio::select! {
            exited = child.wait() => status = Some(exited),
            () = tokio::time::sleep(options.timeout) => timed_out = true,
            Ok(hit) = &mut hit_receiver => limit = Some(hit),
        }
        // Nothing of the command outlives it: what it left in the background goes now.
        let root_running = status.is_none();
        running.root_running = root_running;
        let all_processes_ended = if !root_running && tracker.is_empty() {
            true
        } else {
            let tracker = tracker.clone();
            tokio::task::spawn_blocking(move || tracker.kill(root_running)).await.unwrap_or(false)
        };
        let status = match status {
            Some(status) => status,
            None => child.wait().await,
        }
        .map_err(|e| Error::io("Waiting for the command", e))?;
        running.finished = true;
        drop(running);
        // A limit found while the shell was already ending still counts.
        if limit.is_none() {
            limit = hit_receiver.try_recv().ok();
        }

        // With every process gone the pipes are at their end; the wait covers a process that
        // could not be killed.
        let (stdout, stderr) = tokio::join!(collect(stdout_task, stdout, cap), collect(stderr_task, stderr, cap));

        use std::os::unix::process::ExitStatusExt as _;
        Ok(Output {
            stdout,
            stderr,
            exit_code: status.code(),
            signal: status.signal(),
            timed_out,
            limit,
            duration: started.elapsed(),
            all_processes_ended,
        })
    }

    /// What [`run_blocking`] returns.
    pub(crate) struct Finished {
        pub(crate) exit_code: Option<i32>,
        pub(crate) stdout: Vec<u8>,
        pub(crate) stderr: Vec<u8>,
    }

    /// Runs a command without a runtime, for the self-test behind [`crate::support`]. The
    /// command's output must fit a pipe's buffer.
    pub(crate) fn run_blocking(spec: &Spec<'_>, wait: Duration) -> Result<Finished, Error> {
        use std::io::Read as _;
        let (mut command, prepared) = platform::prepare(spec)?;
        command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => return Err(prepared.spawn_failed(error)),
        };
        drop(command);
        let tracker = match prepared.started(child.id(), spec.folder) {
            Ok(tracker) => tracker,
            Err(error) => {
                platform::kill_group(child.id());
                let _ = child.wait();
                return Err(error);
            }
        };
        let deadline = Instant::now() + wait;
        let status = loop {
            tracker.service(Duration::from_millis(5));
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() < deadline => {}
                Ok(None) => {
                    tracker.kill(true);
                    let _ = child.wait();
                    return Err(Error::Sandbox("the self-test command did not finish".into()));
                }
                Err(e) => return Err(Error::io("Waiting for the self-test command", e)),
            }
        };
        if !tracker.is_empty() {
            tracker.kill(false);
        }
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        if let Some(mut pipe) = child.stdout.take() {
            let _ = pipe.read_to_end(&mut stdout);
        }
        if let Some(mut pipe) = child.stderr.take() {
            let _ = pipe.read_to_end(&mut stderr);
        }
        Ok(Finished { exit_code: status.code(), stdout, stderr })
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) use imp::run_blocking;

#[cfg(test)]
mod tests {
    use super::Capture;

    #[test]
    fn output_under_the_cap_is_kept_whole() {
        let mut capture = Capture::new(10);
        capture.push(b"hello");
        capture.push(b"12345");
        let captured = capture.finish();
        assert_eq!((captured.text.as_str(), captured.tail.as_str()), ("hello12345", ""));
        assert_eq!((captured.omitted_bytes, captured.total_bytes), (0, 10));
        assert!(!captured.truncated());
    }

    #[test]
    fn output_over_the_cap_keeps_both_ends() {
        let mut capture = Capture::new(10);
        capture.push(b"abcdefgh");
        capture.push(b"ijklmnop");
        capture.push(b"qrstuvwxyz");
        let captured = capture.finish();
        assert_eq!((captured.text.as_str(), captured.tail.as_str()), ("abcde", "vwxyz"));
        assert_eq!((captured.omitted_bytes, captured.total_bytes), (16, 26));
        assert!(captured.truncated());
    }

    #[test]
    fn a_cut_inside_a_character_drops_the_character() {
        // "é" is two bytes; the caps fall inside one at both ends.
        let mut capture = Capture::new(6);
        capture.push("ééééé".as_bytes());
        let captured = capture.finish();
        assert_eq!((captured.text.as_str(), captured.tail.as_str()), ("é", "é"));
    }
}
