//! Shell commands and file edits for one sample, confined to a scratch folder.
//!
//! A run that lets the model work as coding harnesses do (run commands, edit files) runs
//! model-written commands on the user's own computer, for every sample. This crate makes that
//! safe to offer:
//!
//! - [`Workspaces`] and [`Workspace`]: a scratch folder per sample, deleted afterwards, with a
//!   short path that has the same shape on every run and no user name in it.
//! - [`Workspace::run`]: one command, with bash, inside the operating system's sandbox. It cannot
//!   write outside the sample's folder, read the user's files, use the network (unless the caller
//!   turns it on) or outlive its timeout and limits. The environment is a fixed short list.
//! - [`Workspace::read`], [`write`](Workspace::write), [`edit`](Workspace::edit) and
//!   [`list`](Workspace::list): file helpers that cannot leave the workspace, also through
//!   symlinks.
//! - [`support`]: which mechanism this machine has and what it enforces.
//!
//! There is no unsandboxed mode. Where the sandbox is missing (Windows, a kernel before Linux 6.2,
//! a container that blocks seccomp), [`Workspace::run`] returns [`Error::Unsupported`] with the
//! reason, and the file helpers still work.
//!
//! | Platform | Mechanism |
//! | --- | --- |
//! | Linux | Landlock and seccomp, put on the child before it starts; a supervisor thread answers the calls neither can decide alone |
//! | macOS | Seatbelt, through `/usr/bin/sandbox-exec` with a profile made per command |
//! | Windows | none: commands are refused |
//!
//! `docs/shell-sandbox.md` lists what is enforced on each platform and what is not.
//!
//! ```no_run
//! # async fn example() -> Result<(), stellarator_shell::Error> {
//! use stellarator_shell::{RunOptions, WorkspaceOptions, Workspaces};
//!
//! let workspaces = Workspaces::open_default()?; // once, at startup
//! let workspace = workspaces.create(&WorkspaceOptions::default())?; // per sample
//! workspace.write("hello.sh", "echo hello from $PWD\n")?;
//! let output = workspace.run("bash hello.sh", &RunOptions::default()).await?;
//! assert_eq!(output.exit_code, Some(0));
//! // Dropping the workspace deletes its folder.
//! # Ok(())
//! # }
//! ```
//!
//! # For the caller
//!
//! - Call [`support`] once at startup, on a blocking thread: the first call runs a command through
//!   the sandbox to see that it works here (a few milliseconds on Linux, a few `sandbox-exec`
//!   starts on macOS). [`Workspace::run`] calls it too, and would do that work on its own first
//!   call.
//! - Every command starts a new shell in the workspace: a `cd` or a variable set in one command is
//!   gone in the next, and so is every process the command left running.
//! - [`Workspace::run`] is async and holds one thread while its command runs. The file helpers
//!   are blocking calls.
//! - A command's output can hold the computer's host name (`uname -a`, `hostname`); the user's
//!   name is kept out of it (`ls -l` shows a number). `docs/shell-sandbox.md`, "What is not
//!   enforced".

mod error;
mod files;
mod run;
mod support;
mod workspace;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(unix)]
mod unix;

#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(target_os = "macos")]
use macos as platform;

pub use error::Error;
pub use files::{DirEntry, EntryKind, FileLimits, Listing};
pub use run::{Captured, LimitHit, Limits, Output, RunOptions};
pub use support::{support, Enforced, Mechanism, Support};
pub use workspace::{Workspace, WorkspaceOptions, Workspaces};
