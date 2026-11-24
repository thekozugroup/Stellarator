//! The seccomp filter: what Landlock does not cover.
//!
//! The filter is a denylist over the native system call table. Three answers:
//!
//! - an error, for the network, for other ways out (io_uring, ptrace, mounts, namespaces, the
//!   kernel keyring, ...) and for extended attributes;
//! - the supervisor (`notify.rs`), for the calls whose answer depends on which file or process
//!   they name: `chmod`, `chown` and `utimes` (Landlock does not cover them), `setsid` (so that
//!   every session of the command is known when it is killed) and, before Landlock ABI 6, signals;
//! - allowed, for everything else.
//!
//! Unix sockets are closed off entirely, not only with the network off: Landlock does not check
//! `connect` on a socket path, and the session's D-Bus, a Docker socket or an SSH agent would be a
//! way to run things outside the sandbox. `socketpair` stays, for pipes between a command's own
//! processes.

use libc::{c_long, sock_filter};

// Classic BPF.
const LD_ABS: u16 = 0x20; // BPF_LD | BPF_W | BPF_ABS
const JEQ: u16 = 0x15; // BPF_JMP | BPF_JEQ | BPF_K
const JGE: u16 = 0x35; // BPF_JMP | BPF_JGE | BPF_K
const JSET: u16 = 0x45; // BPF_JMP | BPF_JSET | BPF_K
const AND: u16 = 0x54; // BPF_ALU | BPF_AND | BPF_K
const RET: u16 = 0x06; // BPF_RET | BPF_K

// Offsets in `struct seccomp_data` (little-endian: the low half of an argument comes first).
const OFF_NR: u32 = 0;
const OFF_ARCH: u32 = 4;
const fn arg(n: u32) -> u32 {
    16 + 8 * n
}

const ALLOW: u32 = 0x7fff_0000;
const KILL_PROCESS: u32 = 0x8000_0000;
const USER_NOTIF: u32 = 0x7fc0_0000;
const fn errno(code: i32) -> u32 {
    0x0005_0000 | (code as u32 & 0xffff)
}

#[cfg(target_arch = "x86_64")]
const NATIVE_ARCH: u32 = 0xc000_003e; // AUDIT_ARCH_X86_64
#[cfg(target_arch = "aarch64")]
const NATIVE_ARCH: u32 = 0xc000_00b7; // AUDIT_ARCH_AARCH64

/// Whether seccomp filters are written for this processor.
pub(crate) const ARCH_SUPPORTED: bool = cfg!(any(target_arch = "x86_64", target_arch = "aarch64"));

// System calls added after the tables were unified (the same number on every architecture), by
// number: not every libc version names them.
pub(crate) const SYS_PIDFD_SEND_SIGNAL: c_long = 424;
const SYS_IO_URING_SETUP: c_long = 425;
const SYS_IO_URING_ENTER: c_long = 426;
const SYS_IO_URING_REGISTER: c_long = 427;
const SYS_OPEN_TREE: c_long = 428;
const SYS_MOVE_MOUNT: c_long = 429;
const SYS_FSOPEN: c_long = 430;
const SYS_FSCONFIG: c_long = 431;
const SYS_FSMOUNT: c_long = 432;
const SYS_FSPICK: c_long = 433;
const SYS_CLONE3: c_long = 435;
const SYS_MOUNT_SETATTR: c_long = 442;
const SYS_QUOTACTL_FD: c_long = 443;
pub(crate) const SYS_FCHMODAT2: c_long = 452;
const SYS_SETXATTRAT: c_long = 463;
const SYS_REMOVEXATTRAT: c_long = 466;
const SYS_OPEN_TREE_ATTR: c_long = 467;
const SYS_FILE_SETATTR: c_long = 469;

/// `CLONE_NEWNS | NEWCGROUP | NEWUTS | NEWIPC | NEWUSER | NEWPID | NEWNET`.
const CLONE_NAMESPACES: u32 = 0x7e02_0000;

fn stmt(code: u16, k: u32) -> sock_filter {
    sock_filter { code, jt: 0, jf: 0, k }
}

fn jump(code: u16, k: u32, jt: u8, jf: u8) -> sock_filter {
    sock_filter { code, jt, jf, k }
}

/// A filter under construction. Every block starts and ends with the system call number in the
/// accumulator; a block that loads an argument returns instead of falling through.
struct Program(Vec<sock_filter>);

impl Program {
    /// `nr` gets `action`.
    fn on(&mut self, nr: c_long, action: u32) {
        self.0.push(jump(JEQ, nr as u32, 0, 1));
        self.0.push(stmt(RET, action));
    }

    fn deny(&mut self, calls: &[c_long], code: i32) {
        for nr in calls {
            self.on(*nr, errno(code));
        }
    }

    fn notify(&mut self, calls: &[c_long]) {
        for nr in calls {
            self.on(*nr, USER_NOTIF);
        }
    }

    /// A block for one system call: `body` runs when the number matches and must return.
    fn block(&mut self, nr: c_long, body: Vec<sock_filter>) {
        self.0.push(jump(JEQ, nr as u32, 0, body.len() as u8));
        self.0.extend(body);
    }
}

/// The calls the supervisor answers because they change a file's mode, owner or times.
pub(crate) fn metadata_calls() -> Vec<c_long> {
    let mut calls = vec![
        libc::SYS_fchmod,
        libc::SYS_fchmodat,
        SYS_FCHMODAT2,
        libc::SYS_fchown,
        libc::SYS_fchownat,
        libc::SYS_utimensat,
    ];
    #[cfg(target_arch = "x86_64")]
    calls.extend([
        libc::SYS_chmod,
        libc::SYS_chown,
        libc::SYS_lchown,
        libc::SYS_utime,
        libc::SYS_utimes,
        libc::SYS_futimesat,
    ]);
    calls
}

/// The calls that send a signal to a process named by number.
pub(crate) fn signal_calls() -> Vec<c_long> {
    vec![
        libc::SYS_kill,
        libc::SYS_tkill,
        libc::SYS_tgkill,
        libc::SYS_rt_sigqueueinfo,
        libc::SYS_rt_tgsigqueueinfo,
        SYS_PIDFD_SEND_SIGNAL,
    ]
}

/// The filter for one command. `network`: the caller turned the network on. `mediate_signals`:
/// the kernel's Landlock cannot scope signals, so the supervisor answers them.
pub(crate) fn program(network: bool, mediate_signals: bool) -> Vec<sock_filter> {
    let mut p = Program(Vec::with_capacity(256));

    // Only the native table: a filter written for it means nothing for another one, so a 32-bit
    // program (or the x32 table on x86-64) is ended at its first call.
    p.0.push(stmt(LD_ABS, OFF_ARCH));
    p.0.push(jump(JEQ, NATIVE_ARCH, 1, 0));
    p.0.push(stmt(RET, KILL_PROCESS));
    p.0.push(stmt(LD_ABS, OFF_NR));
    #[cfg(target_arch = "x86_64")]
    {
        p.0.push(jump(JGE, 0x4000_0000, 0, 1));
        p.0.push(stmt(RET, KILL_PROCESS));
    }

    // ---- The network ----------------------------------------------------------------------
    let deny = stmt(RET, errno(libc::EPERM));
    let allow = stmt(RET, ALLOW);
    if network {
        // IPv4, IPv6 and the routing socket that getaddrinfo and `ip` read; nothing else.
        p.block(
            libc::SYS_socket,
            vec![
                stmt(LD_ABS, arg(0)),
                jump(JEQ, libc::AF_INET as u32, 5, 0),
                jump(JEQ, libc::AF_INET6 as u32, 4, 0),
                jump(JEQ, libc::AF_NETLINK as u32, 0, 2),
                stmt(LD_ABS, arg(2)),
                jump(JEQ, libc::NETLINK_ROUTE as u32, 1, 0),
                deny,
                allow,
            ],
        );
    } else {
        p.deny(
            &[
                libc::SYS_socket,
                libc::SYS_connect,
                libc::SYS_bind,
                libc::SYS_listen,
                libc::SYS_accept,
                libc::SYS_accept4,
            ],
            libc::EPERM,
        );
    }
    // A pair of connected Unix sockets, except datagram ones: a datagram socket can be pointed at
    // another socket's path afterwards.
    p.block(
        libc::SYS_socketpair,
        vec![
            stmt(LD_ABS, arg(0)),
            jump(JEQ, libc::AF_UNIX as u32, 0, 3),
            stmt(LD_ABS, arg(1)),
            stmt(AND, 0xf),
            jump(JEQ, libc::SOCK_DGRAM as u32, 0, 1),
            deny,
            allow,
        ],
    );

    // ---- Other ways out -------------------------------------------------------------------
    // "Not implemented" where libraries fall back on it: io_uring (which would make sockets
    // without `socket`), clone3 (its flags are in memory the filter cannot read; glibc falls back
    // to clone, whose flags are checked below) and the kernel keyring.
    p.deny(
        &[
            SYS_IO_URING_SETUP,
            SYS_IO_URING_ENTER,
            SYS_IO_URING_REGISTER,
            SYS_CLONE3,
            libc::SYS_add_key,
            libc::SYS_request_key,
            libc::SYS_keyctl,
        ],
        libc::ENOSYS,
    );
    // New namespaces: a user namespace hands out capabilities inside it.
    p.block(
        libc::SYS_clone,
        vec![stmt(LD_ABS, arg(0)), jump(JSET, CLONE_NAMESPACES, 0, 1), deny, allow],
    );
    p.deny(
        &[
            libc::SYS_unshare,
            libc::SYS_setns,
            libc::SYS_ptrace,
            libc::SYS_process_vm_readv,
            libc::SYS_process_vm_writev,
            libc::SYS_kcmp,
            libc::SYS_mount,
            libc::SYS_umount2,
            SYS_OPEN_TREE,
            SYS_OPEN_TREE_ATTR,
            SYS_MOVE_MOUNT,
            SYS_FSOPEN,
            SYS_FSCONFIG,
            SYS_FSMOUNT,
            SYS_FSPICK,
            SYS_MOUNT_SETATTR,
            libc::SYS_pivot_root,
            libc::SYS_chroot,
            libc::SYS_name_to_handle_at,
            libc::SYS_open_by_handle_at,
            libc::SYS_bpf,
            libc::SYS_perf_event_open,
            libc::SYS_userfaultfd,
            libc::SYS_fanotify_init,
            libc::SYS_kexec_load,
            libc::SYS_kexec_file_load,
            libc::SYS_init_module,
            libc::SYS_finit_module,
            libc::SYS_delete_module,
            libc::SYS_reboot,
            libc::SYS_swapon,
            libc::SYS_swapoff,
            libc::SYS_acct,
            libc::SYS_quotactl,
            SYS_QUOTACTL_FD,
            libc::SYS_syslog,
            libc::SYS_settimeofday,
            libc::SYS_clock_settime,
            libc::SYS_clock_adjtime,
            libc::SYS_adjtimex,
            libc::SYS_sethostname,
            libc::SYS_setdomainname,
            SYS_FILE_SETATTR,
        ],
        libc::EPERM,
    );

    // ---- What Landlock leaves open on files ---------------------------------------------------
    // Extended attributes: "not supported", which cp, tar and rsync take quietly.
    p.deny(
        &[
            libc::SYS_setxattr,
            libc::SYS_lsetxattr,
            libc::SYS_fsetxattr,
            libc::SYS_removexattr,
            libc::SYS_lremovexattr,
            libc::SYS_fremovexattr,
            SYS_SETXATTRAT,
            SYS_REMOVEXATTRAT,
        ],
        libc::ENOTSUP,
    );
    p.notify(&metadata_calls());

    // ---- Processes --------------------------------------------------------------------------
    p.notify(&[libc::SYS_setsid]);
    if mediate_signals {
        p.notify(&signal_calls());
    }

    p.0.push(allow);
    p.0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every jump lands inside the program, and the program ends by returning.
    #[test]
    fn the_filter_is_well_formed() {
        for (network, signals) in [(false, false), (false, true), (true, false), (true, true)] {
            let program = program(network, signals);
            assert!(program.len() < 4096);
            for (i, ins) in program.iter().enumerate() {
                if ins.code & 0x07 == 0x05 {
                    let after = i + 1;
                    assert!(after + (ins.jt as usize) < program.len(), "jt of {i}");
                    assert!(after + (ins.jf as usize) < program.len(), "jf of {i}");
                }
            }
            assert_eq!(program.last().map(|i| (i.code, i.k)), Some((RET, ALLOW)));
        }
    }
}
