//! Shared by the test files: a folder for test workspaces, files placed outside it, and a way to
//! look for processes a command left behind.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use stellarator_shell::{Output, RunOptions, Workspace, WorkspaceOptions, Workspaces};

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// A number no other test of this run has.
pub fn unique() -> u32 {
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Where the tests keep their workspaces: beside the real ones, never in them.
pub fn base() -> PathBuf {
    // For running the tests as a second user on one machine.
    if let Some(base) = std::env::var_os("STELLARATOR_SHELL_TEST_BASE") {
        return PathBuf::from(base);
    }
    if cfg!(target_os = "macos") {
        PathBuf::from("/private/tmp/stellarator-test")
    } else if cfg!(unix) {
        PathBuf::from("/var/tmp/stellarator-test")
    } else {
        std::env::temp_dir().join("stellarator-test")
    }
}

pub fn workspaces() -> Workspaces {
    Workspaces::open(base()).expect("the test workspace folder opens")
}

pub fn workspace() -> Workspace {
    workspaces().create(&WorkspaceOptions::default()).expect("a workspace is created")
}

/// A folder of its own for a test about slots and sweeping, deleted when dropped.
pub struct OwnBase(pub PathBuf);

impl OwnBase {
    pub fn new(name: &str) -> Self {
        let mut path = base().into_os_string();
        path.push(format!("-{name}-{}", std::process::id()));
        let path = PathBuf::from(path);
        let _ = std::fs::remove_dir_all(&path);
        Self(path)
    }
}

impl Drop for OwnBase {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A folder outside every workspace, with a secret in it: what a command must not reach.
/// `in_home` puts it in the user's real home folder instead of the temporary folder.
pub struct Outside {
    pub dir: PathBuf,
    pub secret: PathBuf,
}

pub const SECRET: &str = "TOP-SECRET-7f3a9c";

impl Outside {
    pub fn new(in_home: bool) -> Self {
        let name = format!("stellarator-shell-test-{}-{}", std::process::id(), unique());
        let parent = if in_home {
            PathBuf::from(std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).expect("a home folder"))
        } else {
            std::env::temp_dir()
        };
        let dir = std::fs::canonicalize(&parent).expect("the parent folder exists").join(name);
        std::fs::create_dir(&dir).expect("the outside folder is created");
        let secret = dir.join("secret.txt");
        std::fs::write(&secret, SECRET).expect("the secret is written");
        #[cfg(unix)]
        {
            // Whatever the umask: the tests compare the mode afterwards.
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o644)).expect("the mode is set");
        }
        Self { dir, secret }
    }

    /// The names in the folder, sorted.
    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&self.dir)
            .expect("the outside folder is listed")
            .map(|e| e.expect("an entry").file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }
}

impl Drop for Outside {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The ways the sandbox is set up on this machine, each to be proven: on Linux also as a kernel
/// with Landlock ABI 3 would do it (signals checked by the supervisor, the network by seccomp
/// alone).
pub fn variants() -> Vec<(&'static str, RunOptions)> {
    let quick = RunOptions { timeout: Duration::from_secs(20), ..RunOptions::default() };
    let mut variants = vec![("default", quick.clone())];
    if cfg!(target_os = "linux") {
        variants.push(("landlock-abi-3", RunOptions { landlock_abi: Some(3), ..quick }));
    }
    variants
}

pub async fn run(workspace: &Workspace, command: &str, options: &RunOptions) -> Output {
    workspace.run(command, options).await.unwrap_or_else(|e| panic!("`{command}` did not run: {e}"))
}

/// stdout and stderr together, for assertions and failure messages.
pub fn text(output: &Output) -> String {
    format!("{}{}{}{}", output.stdout.text, output.stdout.tail, output.stderr.text, output.stderr.tail)
}

/// A `sleep` duration that marks its process: no other process has it on its command line.
pub fn marker() -> String {
    format!("100000.{}{:04}", std::process::id() % 100_000, unique())
}

/// Processes (seen from outside the sandbox) whose command line has `marker`.
#[cfg(unix)]
pub fn processes_with(marker: &str) -> Vec<i32> {
    let listing = std::process::Command::new("ps")
        .args(["-axww", "-o", "pid=,command="])
        .output()
        .expect("ps runs");
    String::from_utf8_lossy(&listing.stdout)
        .lines()
        .filter(|line| line.contains(marker))
        .filter_map(|line| line.split_whitespace().next()?.parse().ok())
        .collect()
}

/// Waits until no process has `marker`, and returns those still there after 5 seconds.
#[cfg(unix)]
pub async fn survivors(marker: &str) -> Vec<i32> {
    for _ in 0..50 {
        if processes_with(marker).is_empty() {
            return Vec::new();
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    processes_with(marker)
}

pub fn mode(path: &Path) -> u32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::metadata(path).expect("the file is there").permissions().mode() & 0o7777
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        0
    }
}
