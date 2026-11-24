//! Reading `/proc`, from Stellarator's own process: which processes belong to a command.

use std::fs;

/// The fields of `/proc/<pid>/stat` the sandbox uses.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Stat {
    /// `R`, `S`, `Z` (zombie), `X` (dead), ...
    pub(crate) state: u8,
    pub(crate) pgrp: i32,
    pub(crate) session: i32,
}

/// `/proc/<pid>/stat` of a process or thread, or `None` when it is gone.
pub(crate) fn stat(pid: i32) -> Option<Stat> {
    let text = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // "pid (comm) state ppid pgrp session ...": the name can hold spaces and parentheses, so the
    // fields start after the last ")".
    let rest = &text[text.rfind(')')? + 1..];
    let mut fields = rest.split_ascii_whitespace();
    let state = *fields.next()?.as_bytes().first()?;
    let _ppid = fields.next()?;
    let pgrp = fields.next()?.parse().ok()?;
    let session = fields.next()?.parse().ok()?;
    Some(Stat { state, pgrp, session })
}

/// Every process number in `/proc`.
pub(crate) fn pids() -> Vec<i32> {
    let Ok(entries) = fs::read_dir("/proc") else { return Vec::new() };
    entries.flatten().filter_map(|e| e.file_name().to_str()?.parse().ok()).collect()
}

fn status_field(status: &str, name: &str) -> Option<u64> {
    let line = status.lines().find(|l| l.starts_with(name))?;
    line[name.len()..].split_ascii_whitespace().next()?.parse().ok()
}

/// The process a thread belongs to.
pub(crate) fn tgid(tid: i32) -> Option<i32> {
    let status = fs::read_to_string(format!("/proc/{tid}/status")).ok()?;
    status_field(&status, "Tgid:").map(|v| v as i32)
}

/// Whether the process runs under `no_new_privs` and a seccomp filter, as every process of a
/// command does and cannot undo. Checked before a process found by its session is killed, in case
/// the session's number was reused by an unrelated process.
pub(crate) fn is_sandboxed(pid: i32) -> bool {
    let Ok(status) = fs::read_to_string(format!("/proc/{pid}/status")) else { return false };
    status_field(&status, "NoNewPrivs:") == Some(1) && status_field(&status, "Seccomp:") == Some(2)
}

/// The process a pidfd of `tid` (descriptor `fd` in its table) refers to.
pub(crate) fn pidfd_target(tid: i32, fd: i32) -> Option<i32> {
    let info = fs::read_to_string(format!("/proc/{tid}/fdinfo/{fd}")).ok()?;
    let line = info.lines().find(|l| l.starts_with("Pid:"))?;
    line[4..].trim().parse().ok()
}

/// How many tasks (threads) run as `uid`: what the kernel compares `RLIMIT_NPROC` with.
pub(crate) fn user_tasks(uid: u32) -> u64 {
    let mut tasks = 0;
    for pid in pids() {
        let Ok(status) = fs::read_to_string(format!("/proc/{pid}/status")) else { continue };
        if status_field(&status, "Uid:") == Some(u64::from(uid)) {
            tasks += status_field(&status, "Threads:").unwrap_or(1);
        }
    }
    tasks
}
