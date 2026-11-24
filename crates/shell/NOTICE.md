# Third-party work in this crate

| File | What it is | License |
| --- | --- | --- |
| `src/macos/profile.sbpl` | The Seatbelt profile. Adapted from OpenAI's Codex CLI (github.com/openai/codex, commit `44308610ef5cbc03d149707735878b0c8743b2df`): `codex-rs/sandboxing/src/seatbelt_base_policy.sbpl` and `codex-rs/sandboxing/src/seatbelt_read_only_platform_defaults.sbpl`. | Apache-2.0, Copyright 2025 OpenAI, `LICENSE-codex` |
| The `NETWORK` text in `src/macos/mod.rs` | The services a command needs for name lookups and TLS. Adapted from `codex-rs/sandboxing/src/seatbelt_network_policy.sbpl` of the same commit. | Apache-2.0, Copyright 2025 OpenAI, `LICENSE-codex` |
| Everything else | Stellarator's. | Stellarator's MIT license |

Codex's NOTICE file reads: "OpenAI Codex, Copyright 2025 OpenAI". Its part about Ratatui concerns
code this crate does not use.

The profile was modified:

- Codex's two files are one profile here, and the rules for terminals (`/dev/ptmx`, `/dev/ttys*`)
  are left out: a command has no terminal.
- Two lookups are left out so that a command does not see the user's name: the directory service
  `com.apple.system.opendirectoryd.libinfo` and the per-user temporary folder
  `com.apple.bsd.dirhelper`.
- `/private/etc`, `/Library` and `/private/var/db` are lists of files and folders instead of whole
  folders; `/Library/Preferences` is not readable.
- Added: Homebrew (`/opt/homebrew`, `/usr/local`) without its `var` and `etc`, the developer tools
  (`/Library/Developer/CommandLineTools`, Xcode's `Developer` folder), `/System/Cryptexes`,
  `/private/var/select`, and the sample's folder as the one writable place.
- Added: `file-map-executable` for every readable file, `sudo`, `su` and other set-user-ID
  programs refused, Unix socket pairs allowed.
- The network rules allow IPv4 and IPv6 by address and the resolver's socket, not every Unix
  socket.

Nothing was copied from Anthropic's sandbox-runtime (github.com/anthropic-experimental/sandbox-runtime,
Apache-2.0). Three of its rule spellings were used as a reference for what `sandbox-exec` accepts:
`(remote ip "*:*")`, `(local ip "*:*")` and `(allow system-socket (socket-domain AF_UNIX))`.

The Linux sandbox is written from the kernel's documentation (Landlock, seccomp user notification).
Codex's `codex-rs/linux-sandbox/src/seccomp.rs` was read for which system calls to refuse; no code
of it is used.
