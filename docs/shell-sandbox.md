# The shell sandbox

`crates/shell` runs shell commands and file edits for one sample in a scratch folder, inside the
operating system's sandbox. It is the foundation for runs where the model works as coding harnesses
do: every sample of such a run executes model-written commands on the user's own computer.

This page says what is enforced on each platform, what is not, and how each rule is proven. The
crate is not wired into runs yet.

- [What a command gets](#what-a-command-gets)
- [What is enforced](#what-is-enforced)
- [What is not enforced](#what-is-not-enforced)
- [Limits and defaults](#limits-and-defaults)
- [Cost](#cost)
- [Linux](#linux)
- [macOS](#macos)
- [Windows](#windows)
- [After a command](#after-a-command)
- [File helpers](#file-helpers)
- [Proof](#proof)

There is no unsandboxed mode. Where the sandbox is missing, running a command is an error that
says why (`Error::Unsupported`), and `support()` reports the same reason beforehand.

## What a command gets

**A workspace per sample.** `Workspaces::create` makes a folder for one sample, empty or as a copy
of a template folder, and deletes it when the `Workspace` is dropped. Folders left by a process
that died are deleted when the next process opens the same base.

| Platform | Base | Why there |
| --- | --- | --- |
| Linux | `/var/tmp/stellarator` | On disk everywhere. `/tmp` is memory on Debian 13 and Fedora, so a workspace there would use RAM. `$TMPDIR` and `~/.cache` hold the user's name. |
| macOS | `/private/tmp/stellarator` | The real path of `/tmp`, so `pwd` and `pwd -P` print the same. `$TMPDIR` is a long per-user path under `/var/folders`. |
| Windows | `%ProgramData%\Stellarator\workspaces` | No user name in it. Commands do not run on Windows; the file helpers do. |

```text
/var/tmp/stellarator/
  .locks/3            held by the operating system while slot 3 is in use
  3/                  one sample; the one place its commands may write
    workspace/        the working directory, and the file helpers' root
    home/             HOME
    tmp/              TMPDIR
```

A command's working directory is `/var/tmp/stellarator/<n>/workspace`. `<n>` is the lowest free
number, so with 8 samples in flight every path a model sees is one of eight, with the same shape on
every run and no user name. `HOME` and `TMPDIR` are beside the workspace, not in it, so that caches
and dot files do not show up in `ls -a` or get added by `git add -A`. When another user of the
computer owns the base, `Workspaces::open_default` uses `/var/tmp/stellarator-<uid>` instead. The
base can also be set by the caller.

**The shell.** `bash -c <command>` (`/bin/bash`, `/usr/bin/bash` or `/usr/local/bin/bash`); `sh`
where there is no bash. Standard input is empty.

**The environment.** A fixed list, with nothing from Stellarator's own environment: tokens such as
`HF_TOKEN` never reach a command.

| Variable | Value |
| --- | --- |
| `PATH` | Linux: `/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin`. macOS: `/opt/homebrew/bin:/opt/homebrew/sbin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin` |
| `HOME`, `TMPDIR`, `PWD` | The sample's `home`, `tmp` and `workspace` folders. The caller cannot change these |
| `USER`, `LOGNAME` | `user` |
| `SHELL` | The shell's path |
| `LANG` | Linux: `C.UTF-8`. macOS: `en_US.UTF-8` |
| `TERM`, `TZ` | `dumb`, `UTC` |
| The caller's `RunOptions::env` | Added, or replacing one of the above |

## What is enforced

| Rule | Linux | macOS | Windows |
| --- | --- | --- | --- |
| Mechanism | Landlock, seccomp, and a supervisor thread in Stellarator | Seatbelt, through `/usr/bin/sandbox-exec` | none |
| Runs commands | yes, Linux 6.2 or later on x86-64 and arm64 | yes | no: refused with the reason |
| Writes only in the sample's folder | Landlock | Seatbelt | |
| Mode, owner and dates change only in the sample's folder | supervisor: Landlock does not cover `chmod`, `chown` and `utimes` | Seatbelt | |
| Reads only the sample's folder and the system's programs, libraries and a list of settings files | Landlock | Seatbelt | |
| Not readable: the user's home, other users' homes, mounted volumes, Stellarator's data folder, other samples | yes | yes | |
| No network unless the caller sets `network: true` | seccomp refuses every socket; Landlock also refuses TCP (Linux 6.7) | Seatbelt | |
| With the network on: IPv4 and IPv6 only | seccomp | Seatbelt | |
| No Unix sockets (D-Bus, Docker, SSH agent), network on or off | seccomp; socket pairs work | Seatbelt; socket pairs work | |
| Signals reach the command's own processes only | Landlock (Linux 6.12); the supervisor on older kernels | Seatbelt | |
| Another process's resource limits, priority and processors cannot be changed | supervisor: Landlock does not cover them | Seatbelt (priority; macOS has no call for another process's limits) | |
| System V shared memory, semaphores and message queues | refused by seccomp | refused by number in the profile: Seatbelt's "refuse everything" does not cover them | |
| The environment is the fixed list | yes | yes | |
| No gain of privileges (`sudo`, set-user-ID programs) | `no_new_privs` | Seatbelt refuses set-user-ID programs | |
| A timeout kills every process of the command | process group and every session the command started | process group, session, and every process under the command's profile | |
| Processor time per process | `RLIMIT_CPU` | `RLIMIT_CPU` | |
| Memory per process | `RLIMIT_DATA`: the allocation fails | checked twice a second: the command is stopped | |
| Number of processes per command | supervisor: starting one more fails | checked twice a second: the command is stopped | |
| File size | `RLIMIT_FSIZE`: the write fails with "File too large" | the same | |
| Open files per process | `RLIMIT_NOFILE` | the same | |
| Size of the sample's folder | checked twice a second: the command is stopped | the same | |
| Output kept | capped per stream; the beginning and the end are kept | the same | |

## What is not enforced

These hold on every platform:

- **The folder's size, and on macOS memory and the process count, are checked twice a second**, not
  held by the kernel. A command can pass a cap for up to half a second (longer for a folder so
  large that measuring it takes more than 25 ms: the next check waits twenty times as long as the
  last one took).
- **CPU time and memory are limits per process.** A command with 10 processes can use 10 times the
  memory limit. Only the timeout and the process limit bound the whole command.
- **A command sees the computer's host name**, the user's number (not name), the kernel version and
  the hardware (`uname -a`, `hostname`, `id`). On many Macs the host name holds the owner's first
  name. Output goes into training data, so a run that exports it should replace the host name.
- **Disk and processor use below the limits** is not throttled: a command can keep every core busy
  until its timeout.

Linux:

- **A command can see whether a path outside the workspace exists, and its size and dates**
  (`stat`). Landlock does not cover it. It cannot list a folder or read a file there.
- **Threads are not counted** by the process limit. They are bounded by the memory limit: each
  thread's stack counts toward it.
- **With `show_processes: true`** a command reads the names and command lines of every process on
  the computer, as `ps` shows them. Their environment, memory and open files stay unreadable.
- **`kernel.yama.ptrace_scope`.** The supervisor reads the path that a `chmod` names from the
  caller's memory, which this setting can forbid. With 2 or 3, `chmod`, `chown` and `touch -d` by
  path fail inside the workspace too. With 1 (Ubuntu's default) they fail only for a process whose
  parent has exited, such as the job in `( job & )`: Stellarator may then read its descendants'
  memory only, and an orphan is no longer one.
- **The seccomp filter is a denylist** over the kernel's system calls. A kernel bug in a call that
  stays allowed is not covered.

macOS:

- **`sandbox-exec` is deprecated by Apple.** It works on macOS 26 and is what Claude Code's and
  Codex CLI's sandboxes use. If a release removes it, `support()` reports no sandbox and commands
  are refused; there is no replacement that confines a child process without an entitlement from
  Apple.
- **POSIX semaphores are shared by name** with the user's other programs (Python's multiprocessing
  needs them). A command that knows another program's semaphore name can open it.
- **Finding a command's processes uses `sandbox_check`**, a function of the system's sandbox
  library that is not in Apple's public headers (Chromium and WebKit call it too). `support()`
  tries it on the machine first. Where it does not answer as expected, `setsid` is refused inside
  the sandbox instead, and a process that `posix_spawn` starts in a new session outlives its
  command; `support()` then reports `process_tree_killed: false`.

## Limits and defaults

All are fields of `RunOptions`, `Limits`, `WorkspaceOptions` and `FileLimits`, set per command or
per workspace. `None` turns a limit off.

| Limit | Default | Field |
| --- | --- | --- |
| Timeout | 60 s | `RunOptions::timeout` |
| Network | off | `RunOptions::network` |
| Output kept, per stream | 128 KiB (the first and the last 64 KiB) | `RunOptions::max_output_bytes` |
| Processor time per process | 240 s, then SIGXCPU | `Limits::cpu_seconds` |
| Memory per process | 4 GiB | `Limits::memory_bytes` |
| Processes per command | 256 | `Limits::processes` |
| File size | 256 MiB | `Limits::file_bytes` |
| Open files per process | 1024 | `Limits::open_files` |
| Size of the sample's folder | 1 GiB | `WorkspaceOptions::max_bytes` |
| File helper: read | 1 MiB | `FileLimits::max_read_bytes` |
| File helper: write | 1 MiB | `FileLimits::max_write_bytes` |
| File helper: list | 2,000 entries | `FileLimits::max_list_entries` |
| More readable folders | none | `RunOptions::read_paths` |
| Core files | never written | fixed |

## Cost

`Workspace::run` is async (Tokio, with I/O and time enabled); the file helpers are blocking calls
that take microseconds on small files.

| | Linux | macOS |
| --- | --- | --- |
| One `true`, start to result (median of 20, debug build) | 1.4 ms on GitHub's `ubuntu-latest`, 2.5 ms on a 4-core VM | 30 to 41 ms on `macos-26-intel`, most of it `sandbox-exec` reading the profile |
| Held per running command | one thread (256 KiB stack), three descriptors | one thread, two descriptors |
| While it runs | the supervisor answers `chmod`, `setsid` and process starts: about one round trip between two threads each | every half second: one pass over the process list |
| Commands at once | no cap in the crate. 24 workspaces with 5 commands each, in parallel: 0.22 s in all on the 4-core VM | no cap; the same test passes |

## Linux

Three parts are put on the child process between `fork` and `exec`, so nothing needs to be
installed and no helper program is started.

| Part | What it does |
| --- | --- |
| Landlock | An allowlist of what a command may open. Read and run: `/usr`, `/bin`, `/sbin`, `/lib*`, and `/nix/store` or `/gnu/store` where they exist. Read: a list of files in `/etc` (the loader's, certificates, `hosts`, `resolv.conf`, the time zone, settings of git, pip and others), `/proc/cpuinfo`, `meminfo`, `stat`, `uptime`, `loadavg`, `version`, `/proc/sys`, the processors and cgroups under `/sys`. Read and write: `/dev/null`, `zero`, `full`, `random`, `urandom`. Everything: the sample's folder. |
| seccomp | Refuses sockets, io_uring, ptrace, mounts, new namespaces, the kernel keyring, System V and POSIX message-queue calls, extended attributes and 32-bit system calls. |
| Supervisor | A thread per command that answers the calls neither can decide alone (seccomp user notification): `chmod`, `chown` and `utimes` (done by the supervisor on a file it has checked is in the sample's folder), `setsid` (allowed, and the session recorded for the kill), new processes (counted), another process's limits and priority, and signals before Linux 6.12. |

**Requirements.** Linux 6.2 or later (Landlock ABI 3, the first that covers `truncate`), with
Landlock in the kernel's `lsm=` list, on x86-64 or arm64. The tests ran on x86-64 (Linux 6.18 and
7.0); the arm64 build compiles and has not been run.

| Distribution | Kernel | Sandbox |
| --- | --- | --- |
| Ubuntu 24.04 and later, Debian 13, Fedora 40 and later | 6.8 or later | yes |
| Ubuntu 22.04 with the HWE kernel | 6.5 or later | yes |
| Ubuntu 22.04 with the 5.15 kernel, Debian 12 | 5.15, 6.1 | no: commands are refused |
| Docker | the host's | as the host; not tried in a container for this page |

`support()` runs one command through the sandbox the first time it is called and reports
unavailable, with the reason, if that fails: a container that blocks seccomp or Landlock shows up
there.

**What does not work inside.** These follow from the rules above. A model sees them as errors.

| What | Why | Result |
| --- | --- | --- |
| `ls /`, `ls /etc` | A Landlock rule on `/` would cover every folder | "Permission denied"; files on the list are readable by name |
| `whoami`, owner names in `ls -l` | `/etc/passwd` and `/etc/group` are not readable: they hold the user's name | `ls -l` shows numbers; `whoami` fails |
| `ps`, `top`, `/proc/self/...` | Only machine-wide files of `/proc` are readable, so that other programs' command lines are not | Empty or an error. `show_processes: true` allows all of `/proc`. Java prints a warning about `/proc/self/stat` without it |
| Python's `multiprocessing` locks, pools and queues | They are named semaphores in `/dev/shm`, which every program of the user shares | `PermissionError` |
| Local servers | No sockets with the network off; no Unix sockets ever | With `network: true`, TCP and UDP on `127.0.0.1` work |
| `docker`, `podman`, `unshare`, `strace`, `gdb` | Namespaces and ptrace are refused | "Operation not permitted" |
| `setfattr`, `setfacl`, `cp --preserve=xattr` | Extended attributes are refused | "Operation not supported"; `cp -a` and `tar` continue |
| 32-bit programs | The filter is written for the 64-bit call table | Killed at start |
| Tools installed in the home folder (nvm, pyenv, rustup, conda) | The home folder is not readable | Not found. `read_paths` makes a folder readable, and `env` can add it to `PATH` |

**Running as root.** A command then keeps no capability (the bounding set is emptied), and
`chown` in the workspace may only keep a file's owner. The Docker image runs as a normal user.

## macOS

`sandbox-exec -p <profile>` starts the shell under a Seatbelt profile made per command
(`crates/shell/src/macos/profile.sbpl`, adapted from Codex CLI's, see `crates/shell/NOTICE.md`).
The profile refuses everything and then allows: starting programs, signals to the command's own
processes, a list of machine facts, reading the system's programs and libraries, Homebrew
(`/opt/homebrew`, `/usr/local`, without their `var` and `etc`), the developer tools that
`xcode-select` points at, a list of files in `/private/etc`, and everything in the sample's folder.

**What does not work inside.**

| What | Why | Result |
| --- | --- | --- |
| `ps`, `top`, `sudo` | Seatbelt refuses set-user-ID programs | "Operation not permitted" |
| `whoami`, owner names in `ls -l` | The directory service is not reachable: it holds the user's name | `ls -l` shows the user's number |
| Local servers | No sockets with the network off; no Unix sockets ever | With `network: true`, TCP and UDP on `127.0.0.1` work |
| Tools installed in the home folder | The home folder is not readable | Not found. `read_paths` makes a folder readable |
| A Java from `/usr/bin/java` that lives in the home folder | The same | "Unable to locate a Java Runtime" |

## Windows

Commands do not run on Windows: `support()` reports no sandbox and `Workspace::run` returns
`Error::Unsupported`. The workspaces and the file helpers work.

No mechanism was found that gives the same rules without an installation step by an administrator
or a recent Windows build, and a shell is not part of Windows.

| Mechanism | Writes confined | Reads confined | Network blocked | Needs |
| --- | --- | --- | --- | --- |
| Restricted token (`CreateRestrictedToken` with `WRITE_RESTRICTED`) and a job object | yes, by access lists on the allowed folders | no: a write-restricted token reads what the user reads | no | nothing. Codex CLI's unelevated mode is this; it points proxy variables at a closed port instead of blocking the network |
| A separate local account for commands, with firewall filters on that account | yes | yes | yes | one installation with administrator rights. Codex CLI's elevated mode and Anthropic's sandbox-runtime (`srt-win`) are this |
| AppContainer | yes | yes, by default | by capability | a shell that starts in an AppContainer; not tried. sandbox-runtime does not use MXC's AppContainer tiers and calls their network enforcement cooperative |
| Microsoft MXC, BaseContainer tier | yes | yes | yes | a Windows build that has it (sandbox-runtime measured Windows 11 25H2, build 26200); Microsoft's `mxc-sdk`. Both Codex CLI and sandbox-runtime offer it as an option |

A job object gives the limits on every row: processor time, memory, the number of processes, and
the kill of the whole tree. It has no limit on file size or open files.

What a Windows sandbox to the standard of the other two would take:

1. MXC's BaseContainer where the Windows build has it, and commands refused elsewhere; or the
   separate account with firewall filters, which needs an installer step with administrator
   rights and its removal on uninstall.
2. A shell. Git for Windows' bash is the likely one; sandbox-runtime documents that programs using
   Windows' own TLS (`curl.exe`, cargo) fail under its token. PowerShell would make the commands
   in the training data differ from the other platforms'.
3. The same tests as on Linux and macOS, on the Windows runner.

## After a command

`Workspace::run` returns when every process of the command is gone: what it left in the background
is killed with it, also after a normal exit. A process cannot be left running for a later command
of the same sample.

| Platform | How the processes are found | `Output::all_processes_ended` is `false` when |
| --- | --- | --- |
| Linux | The shell's process group, then every process in a session the command started. The kernel reports when the last process under the command's filter is gone | a process could not be killed within about 2 seconds |
| macOS | The shell's process group, then every process in the shell's session or under the command's profile. Each command's profile allows one name of its own, and the sandbox library says which processes run under a profile that allows it | the same |

Dropping the future that `run` returns (a cancelled sample) kills the command the same way.

## File helpers

`Workspace::read`, `write`, `edit` and `list` run in Stellarator's own process, not in the sandbox,
while a command of the same sample may be running. A command can plant a symlink to any path, so no
path is checked first and opened afterwards: every path is resolved beneath the open workspace
folder by `cap-std` (`openat2` with `RESOLVE_BENEATH` on Linux, one component at a time elsewhere).

| Path | Result |
| --- | --- |
| Relative, or absolute inside the workspace | used |
| `..` that leaves the workspace, an absolute path elsewhere | `Error::OutsideWorkspace` |
| A symlink that leads outside, at any component | `Error::OutsideWorkspace` |
| A symlink that stays inside | followed. One with an absolute target is refused, also when the target is inside |
| A named pipe or a folder where a file is expected | `Error::WrongKind`; the helper does not wait on a pipe |

`edit` replaces one exact string: `Error::NoMatch` when it is not in the file,
`Error::MultipleMatches` with the count when it is there more than once.

## Proof

`crates/shell/tests/sandbox.rs` runs commands inside the sandbox that try to break each rule, and
checks from outside that nothing changed. On Linux each test also runs as a kernel with Landlock
ABI 3 would enforce it (signals by the supervisor, the network by seccomp alone).

| Rule | Test | What it tries |
| --- | --- | --- |
| Reads | `a_command_cannot_read_outside_its_workspace` | `cat`, `cp`, `ls`, a symlink, a hard link, `cd`, a `..` path to a secret in the temporary folder and in the real home; the home, `/home` or `/Users`, `/mnt`, `/Volumes`, the base folder, another sample |
| Writes | `a_command_cannot_write_outside_its_workspace` | 23 ways to change a file outside: write, append, truncate, remove, rename, link, create beside it, `chmod`, `touch`, `utime` |
| Modes and dates inside | `modes_and_dates_work_inside_the_workspace` | `chmod`, `touch -t`, `cp -p`, `tar` and `utime` give the same results as without a sandbox |
| Network | `the_network_is_off_unless_the_caller_turns_it_on` | TCP and UDP to a listener the test holds on `127.0.0.1`, and a Unix socket, with the option off and on |
| Environment | `the_environment_is_a_fixed_list` | `HF_TOKEN` and another variable set in the test process; on Linux also `/proc/<pid>/environ` of the test process |
| Timeout and tree | `a_timeout_kills_the_whole_process_tree` | a background job, an orphan, a process that calls `setsid`, one that `posix_spawn` starts in a new session, one under `nohup`; also a background job after a normal exit, and a dropped run |
| Processes | `processes_cannot_be_spawned_without_end` | a loop that starts 2,000 processes with a limit of 40; 12 commands with 31 processes each at once |
| Output | `output_is_capped` | 3 MB with a cap of 10,000 bytes; `yes` until the timeout |
| Limits | `processor_time_is_limited`, `memory_is_limited`, `file_size_and_open_files_are_limited`, `the_workspace_cannot_grow_past_its_cap` | a busy loop, a 700 MB allocation, a 5 MB file with a 1 MiB limit, 200 open files with a limit of 64, a file that grows 10 MB a second |
| Signals | `a_command_signals_its_own_processes_only` | `kill` of a process outside, by number and by group; on Linux also `prlimit`, `renice`, `taskset`, `chrt` and `ionice` on it |
| Privileges | `a_command_cannot_become_root` | `sudo -n` |
| Shared memory | `system_v_shared_memory_is_closed` | `shmget` |
| File helpers | `tests/files.rs` | `..`, absolute paths, symlinks to a file, a folder, the parent and `/`, a dangling link, a named pipe, the size caps |
| Workspaces | `tests/workspace.rs` | the path's shape, deletion (also of a folder without permissions), the sweep, 16 created at once, a template and its cap |

Where they run: the Linux tests in `cargo test --workspace` on the `ubuntu-latest` runner; the
macOS tests on `macos-26-intel` and the Windows tests on `windows-latest`, in the "Shell sandbox"
job of `.github/workflows/ci.yml`. Each job prints a report of what a command sees inside the
sandbox.

On Linux four of the rules were checked the other way round as well, by taking one answer of the
supervisor out and running the test again. Without its answer for `chmod`, `chmod 777` on a file
outside the workspace succeeded under Landlock alone. Without its answer for signals on ABI 3,
`kill -9` ended a process outside. Without its answer for `prlimit`, a CPU limit of zero was set
on a process outside. Without its record of `setsid`, a process in a session of its own outlived
the timeout.
