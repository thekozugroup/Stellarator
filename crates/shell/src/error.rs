//! The crate's error type.

use std::path::PathBuf;

/// What can go wrong with a workspace, a command or a file helper.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// This machine has no sandbox, so no command runs. There is no unsandboxed mode.
    #[error("Commands cannot run here: {reason}")]
    Unsupported { reason: String },

    /// The sandbox exists but could not be put on this command; the command did not run.
    #[error("The sandbox could not be set up, so the command did not run: {0}")]
    Sandbox(String),

    /// The path leaves the workspace: by `..`, an absolute path elsewhere, or a symlink.
    #[error("{} is outside the workspace", path.display())]
    OutsideWorkspace { path: PathBuf },

    /// An option of the call is not usable: an environment variable's name, a command with a NUL.
    #[error("{0}")]
    InvalidOption(String),

    /// The path is not usable: empty, or with a NUL byte.
    #[error("Not a usable path: {0}")]
    InvalidPath(String),

    /// The file helper found something else where it needs a file or a folder.
    #[error("{} is not {expected}", path.display())]
    WrongKind { path: PathBuf, expected: &'static str },

    /// A file, a result or a folder listing is over its size cap.
    #[error("{what} is {size} bytes; the limit is {limit}")]
    TooLarge { what: String, size: u64, limit: u64 },

    /// The workspace would grow over its size cap.
    #[error("The workspace is full: {size} of {limit} bytes used")]
    WorkspaceFull { size: u64, limit: u64 },

    /// `edit`: the text to replace is not in the file.
    #[error("The text to replace was not found in {}", path.display())]
    NoMatch { path: PathBuf },

    /// `edit`: the text to replace is in the file more than once.
    #[error("The text to replace is in {} {count} times; it must be there once", path.display())]
    MultipleMatches { path: PathBuf, count: usize },

    /// `read_to_string` and `edit`: the file is not UTF-8 text.
    #[error("{} is not UTF-8 text", path.display())]
    NotText { path: PathBuf },

    /// The template folder for a workspace cannot be used.
    #[error("The template folder cannot be used: {0}")]
    Template(String),

    /// The folder that holds the workspaces cannot be used.
    #[error("The workspace folder {} cannot be used: {reason}", base.display())]
    Base { base: PathBuf, reason: String },

    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },
}

impl Error {
    pub(crate) fn io(context: impl Into<String>, source: std::io::Error) -> Self {
        Error::Io { context: context.into(), source }
    }
}

/// Shorthand for `map_err(|e| Error::io(...))`.
pub(crate) trait IoContext<T> {
    fn context(self, context: impl FnOnce() -> String) -> Result<T, Error>;
}

impl<T> IoContext<T> for std::io::Result<T> {
    fn context(self, context: impl FnOnce() -> String) -> Result<T, Error> {
        self.map_err(|e| Error::io(context(), e))
    }
}
