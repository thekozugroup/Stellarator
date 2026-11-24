//! Commands that try to break each rule of the sandbox, run inside it. Linux and macOS; every test
//! needs the sandbox and fails without it.
#![cfg(unix)]

mod common;

use std::io::Read as _;
use std::time::{Duration, Instant};

use common::{marker, run, survivors, text, variants, workspace, workspaces, Outside, SECRET};
use stellarator_shell::{support, LimitHit, Limits, Mechanism, RunOptions, WorkspaceOptions};

/// The sandbox must be there: these tests prove nothing if commands are refused.
fn require_sandbox() {
    let support = support();
    assert!(support.available, "no sandbox on this machine: {:?}", support.reason);
}

fn quick() -> RunOptions {
    RunOptions { timeout: Duration::from_secs(20), ..RunOptions::default() }
}

/// A free port of the range this crate's tests may use, with a listener on it.
fn listener() -> (std::net::TcpListener, u16) {
    for port in 8190..=8199 {
        if let Ok(listener) = std::net::TcpListener::bind(("127.0.0.1", port)) {
            listener.set_nonblocking(true).expect("non-blocking");
            return (listener, port);
        }
    }
    panic!("no free port between 8190 and 8199");
}

/// What a client sent to `listener`, or `None` when nobody connected.
fn received(listener: &std::net::TcpListener) -> Option<String> {
    for _ in 0..10 {
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream.set_nonblocking(false).expect("blocking");
                stream.set_read_timeout(Some(Duration::from_secs(2))).expect("timeout");
                let mut text = String::new();
                let _ = stream.read_to_string(&mut text);
                return Some(text);
            }
            Err(_) => std::thread::sleep(Duration::from_millis(20)),
        }
    }
    None
}

#[tokio::test(flavor = "multi_thread")]
async fn a_command_runs_in_its_workspace() {
    require_sandbox();
    let workspace = workspace();
    for (name, options) in variants() {
        let output = run(&workspace, "echo hello; pwd; echo problem >&2; exit 7", &options).await;
        assert_eq!(output.stdout.text, format!("hello\n{}\n", workspace.path().display()), "{name}");
        assert_eq!(output.stderr.text, "problem\n", "{name}");
        assert_eq!((output.exit_code, output.signal, output.timed_out), (Some(7), None, false), "{name}");
        assert!(output.all_processes_ended, "{name}");

        // Files it writes are in the workspace, and the system's programs are readable.
        let output = run(&workspace, "echo data > made.txt && cat made.txt && ls /usr/bin | head -n 1", &options).await;
        assert_eq!(output.exit_code, Some(0), "{name}: {}", text(&output));
        assert_eq!(std::fs::read_to_string(workspace.path().join("made.txt")).unwrap(), "data\n", "{name}");

        // No input: a command that reads stdin gets the end of it at once.
        let output = run(&workspace, "cat; echo done", &options).await;
        assert_eq!(output.stdout.text, "done\n", "{name}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_environment_is_a_fixed_list() {
    require_sandbox();
    std::env::set_var("STELLARATOR_SHELL_TEST_SECRET", "s3cret-in-the-test-process");
    std::env::set_var("HF_TOKEN", "hf_test_token_in_the_test_process");
    let workspace = workspace();
    for (name, mut options) in variants() {
        options.env = vec![("EXTRA".into(), "given".into()), ("HOME".into(), "/elsewhere".into())];
        let output = run(&workspace, "env; echo \"[$HF_TOKEN][$STELLARATOR_SHELL_TEST_SECRET]\"", &options).await;
        let out = &output.stdout.text;
        assert!(out.contains("[][]"), "{name}: {out}");
        assert!(!out.contains("s3cret") && !out.contains("hf_test_token"), "{name}: {out}");
        assert!(!out.contains("STELLARATOR") && !out.contains("CARGO"), "{name}: {out}");
        assert!(out.contains(&format!("HOME={}\n", workspace.home().display())), "{name}: {out}");
        assert!(out.contains(&format!("TMPDIR={}\n", workspace.tmp().display())), "{name}: {out}");
        assert!(out.contains("EXTRA=given\n") && !out.contains("/elsewhere"), "{name}: {out}");
        let names: Vec<&str> = out.lines().filter_map(|l| l.split_once('=')).map(|(n, _)| n).collect();
        for expected in ["PATH", "HOME", "TMPDIR", "USER", "LOGNAME", "SHELL", "LANG", "TERM", "TZ", "PWD", "EXTRA"] {
            assert!(names.contains(&expected), "{name}: {expected} is missing from {out}");
        }
        // The shell adds SHLVL and _ (and OLDPWD on some); nothing else.
        assert!(names.len() <= 14, "{name}: {out}");

        // HOME and TMPDIR are writable folders of the sample.
        let output = run(&workspace, "echo a > \"$HOME/h\" && echo b > \"$TMPDIR/t\" && cat ~/h \"$TMPDIR/t\"", &options).await;
        assert_eq!(output.stdout.text, "a\nb\n", "{name}: {}", text(&output));
    }

    // Linux: the test process's own environment is in /proc, also when the caller shows processes.
    if cfg!(target_os = "linux") {
        for show_processes in [false, true] {
            let options = RunOptions { show_processes, ..quick() };
            let pid = std::process::id();
            let command = format!("tr '\\0' '\\n' < /proc/{pid}/environ; ls /proc/{pid}/cwd/; ls /proc/{pid}/root/");
            let output = run(&workspace, &command, &options).await;
            assert!(!text(&output).contains("s3cret") && output.stdout.text.is_empty(), "{}", text(&output));
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_command_cannot_read_outside_its_workspace() {
    require_sandbox();
    let workspaces = workspaces();
    let (workspace, other_sample) =
        (workspaces.create(&WorkspaceOptions::default()).unwrap(), workspaces.create(&WorkspaceOptions::default()).unwrap());
    other_sample.write("theirs.txt", SECRET).unwrap();
    let home = std::env::var("HOME").expect("HOME");

    for in_home in [false, true] {
        let outside = Outside::new(in_home);
        let (secret, dir) = (outside.secret.display(), outside.dir.display());
        for (name, options) in variants() {
            let attempts = [
                format!("cat '{secret}'"),
                format!("head -c 100 '{secret}'"),
                format!("cp '{secret}' stolen.txt; cat stolen.txt"),
                format!("ls '{dir}'"),
                format!("ln -s '{secret}' link.txt; cat link.txt"),
                format!("ln '{secret}' hard.txt; cat hard.txt"),
                format!("ln -s '{dir}' linkdir; cat linkdir/secret.txt; ls linkdir/"),
                format!("cd '{dir}' && cat secret.txt"),
                format!("exec 3< '{secret}' && cat <&3"),
                // The same file by a path that climbs out of the workspace.
                format!("cat \"$(pwd)/../../../../../../../..{secret}\""),
            ];
            for attempt in attempts {
                let output = run(&workspace, &attempt, &options).await;
                assert!(!text(&output).contains(SECRET), "{name}: `{attempt}` read the secret: {}", text(&output));
                assert!(!output.stdout.text.contains("secret.txt"), "{name}: `{attempt}` listed the folder");
            }
        }
    }

    for (name, options) in variants() {
        // The user's home, other users' homes, mounted volumes, the folder of all workspaces, and
        // another sample's workspace.
        let other = other_sample.path().display();
        let base = workspaces.base().display();
        for folder in [home.as_str(), "/home", "/Users", "/root", "/mnt", "/media", "/Volumes", &base.to_string()] {
            if !std::path::Path::new(folder).is_dir() {
                continue;
            }
            let output = run(&workspace, &format!("ls -a '{folder}'"), &options).await;
            assert!(output.stdout.text.is_empty() && output.exit_code != Some(0), "{name}: ls {folder}: {}", text(&output));
        }
        let output = run(&workspace, &format!("cat '{other}/theirs.txt'; ls '{other}'; ls ../.."), &options).await;
        assert!(output.stdout.text.is_empty(), "{name}: {}", text(&output));
        // Its own folder is listed: the workspace, home and tmp.
        let output = run(&workspace, "ls ..", &options).await;
        assert_eq!(output.stdout.text, "home\ntmp\nworkspace\n", "{name}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_command_cannot_write_outside_its_workspace() {
    require_sandbox();
    let workspace = workspace();
    for in_home in [false, true] {
        let outside = Outside::new(in_home);
        let (secret, dir) = (outside.secret.display(), outside.dir.display());
        let before = std::fs::metadata(&outside.secret).unwrap();
        for (name, options) in variants() {
            let attempts = [
                format!("echo hacked > '{secret}'"),
                format!("echo hacked >> '{secret}'"),
                format!(": > '{secret}'"),
                format!("cp /dev/null '{secret}'"),
                format!("perl -e 'truncate($ARGV[0], 0) or die \"truncate: $!\"' '{secret}'"),
                format!("rm -f '{secret}'"),
                format!("mv '{secret}' '{secret}.moved'"),
                format!("mv '{secret}' ./taken.txt"),
                format!("ln '{secret}' ./hardlink.txt && echo hacked > ./hardlink.txt"),
                format!("ln -s '{secret}' ./sym.txt; echo hacked > ./sym.txt"),
                format!("ln -s '{dir}' ./symdir; echo hacked > ./symdir/secret.txt; echo new > ./symdir/new.txt"),
                format!("touch '{dir}/new.txt'"),
                format!("mkdir '{dir}/newdir'"),
                format!("ln -s /etc/hosts '{dir}/link'"),
                format!("mkfifo '{dir}/fifo'"),
                // The mode and the dates of a file the user owns: Landlock alone would allow these.
                format!("chmod 777 '{secret}'"),
                format!("chmod 000 '{secret}'"),
                "chmod 777 ./sym.txt".to_string(),
                format!("cd '{dir}' && chmod 600 secret.txt"),
                format!("touch -t 200001010000 '{secret}'"),
                format!("touch '{secret}'"),
                format!("perl -e 'utime(1, 1, $ARGV[0]) or die \"utime: $!\"' '{secret}'"),
                format!("perl -e 'chmod(0777, $ARGV[0]) or die \"chmod: $!\"' '{secret}'"),
            ];
            for attempt in attempts {
                let output = run(&workspace, &attempt, &options).await;
                let after = std::fs::metadata(&outside.secret)
                    .unwrap_or_else(|e| panic!("{name}: `{attempt}` removed the file: {e}"));
                assert_eq!(std::fs::read_to_string(&outside.secret).unwrap(), SECRET, "{name}: `{attempt}` changed the file");
                assert_eq!(outside.names(), ["secret.txt"], "{name}: `{attempt}` changed the folder");
                assert_eq!(common::mode(&outside.secret), 0o644, "{name}: `{attempt}` changed the mode");
                assert_eq!(after.modified().unwrap(), before.modified().unwrap(), "{name}: `{attempt}` changed the date");
                assert!(output.all_processes_ended);
            }
        }
    }

    // Nothing lands in the shared temporary folders either.
    for (name, options) in variants() {
        let stray = format!("stellarator-shell-stray-{}", std::process::id());
        let command = format!("touch /tmp/{stray} /var/tmp/{stray} /dev/shm/{stray}; echo x > /tmp/{stray}");
        run(&workspace, &command, &options).await;
        for folder in ["/tmp", "/var/tmp", "/dev/shm"] {
            assert!(!std::path::Path::new(folder).join(&stray).exists(), "{name}: wrote {folder}/{stray}");
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn modes_and_dates_work_inside_the_workspace() {
    require_sandbox();
    let workspace = workspace();
    for (name, options) in variants() {
        let script = "set -e; rm -rf m; mkdir m; cd m
            echo x > a; chmod 640 a; perl -e 'printf \"%o\\n\", (stat $ARGV[0])[2] & 07777' a
            echo '#!/bin/sh' > run.sh; echo 'echo ran' >> run.sh; chmod +x run.sh; ./run.sh
            touch -t 200001020304 a; perl -e 'print((stat $ARGV[0])[9], \"\\n\")' a
            mkdir sub; chmod 700 sub; cd sub; echo y > b; chmod 600 b; cd ..
            perl -e 'printf \"%o\\n\", (stat $ARGV[0])[2] & 07777' sub/b
            ln -s a alias; chmod 600 alias; perl -e 'printf \"%o\\n\", (stat $ARGV[0])[2] & 07777' a
            cp -p a copy; perl -e 'print((stat $ARGV[0])[9], \"\\n\")' copy
            tar cf t.tar a sub; mkdir out; tar xf t.tar -C out; perl -e 'print((stat $ARGV[0])[9], \"\\n\")' out/a
            perl -e 'utime(86400, 86400, $ARGV[0]) or die; print((stat $ARGV[0])[9], \"\\n\")' sub/b";
        let output = run(&workspace, script, &options).await;
        assert_eq!(output.exit_code, Some(0), "{name}: {}", text(&output));
        // 200001020304 in UTC, the command's time zone.
        assert_eq!(output.stdout.text, "640\nran\n946782240\n600\n600\n946782240\n946782240\n86400\n", "{name}: {}", text(&output));
        assert_eq!(output.stderr.text, "", "{name}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_network_is_off_unless_the_caller_turns_it_on() {
    require_sandbox();
    let workspace = workspace();
    let (listener, port) = listener();
    let connect = format!("exec 3<>/dev/tcp/127.0.0.1/{port} && echo from-the-sandbox >&3 && echo connected");
    for (name, options) in variants() {
        // Off: not even this computer's own address.
        let output = run(&workspace, &connect, &options).await;
        assert!(!output.stdout.text.contains("connected"), "{name}: {}", text(&output));
        assert_ne!(output.exit_code, Some(0), "{name}");
        assert_eq!(received(&listener), None, "{name}: a connection arrived with the network off");

        // On: the same command connects.
        let output = run(&workspace, &connect, &RunOptions { network: true, ..options.clone() }).await;
        assert_eq!(output.stdout.text, "connected\n", "{name}: {}", text(&output));
        assert_eq!(received(&listener).as_deref(), Some("from-the-sandbox\n"), "{name}");
    }

    // Unix sockets of the user's session (D-Bus, Docker, an SSH agent) are not reachable, network
    // on or off.
    let outside = Outside::new(false);
    let socket = std::env::temp_dir().join(format!("stl-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&socket);
    let unix_listener = std::os::unix::net::UnixListener::bind(&socket).expect("a Unix socket");
    unix_listener.set_nonblocking(true).unwrap();
    let connect = format!(
        "perl -MSocket -e 'socket(S, PF_UNIX, SOCK_STREAM, 0) or die \"socket: $!\"; \
         connect(S, sockaddr_un($ARGV[0])) or die \"connect: $!\"; print \"connected\\n\"' '{}'",
        socket.display()
    );
    for (name, options) in variants() {
        for network in [false, true] {
            let output = run(&workspace, &connect, &RunOptions { network, ..options.clone() }).await;
            assert!(!output.stdout.text.contains("connected"), "{name}, network {network}: {}", text(&output));
            assert!(unix_listener.accept().is_err(), "{name}, network {network}: a Unix connection arrived");
        }
    }
    let _ = std::fs::remove_file(&socket);
    drop(outside);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_timeout_kills_the_whole_process_tree() {
    require_sandbox();
    let workspace = workspace();
    for (name, options) in variants() {
        let marks: Vec<String> = (0..5).map(|_| marker()).collect();
        // A background job, an orphan, a process in a session of its own, one under nohup, and the
        // foreground.
        let command = format!(
            "sleep {} &\n(sleep {} &)\n\
             perl -e 'use POSIX; if (fork() == 0) {{ POSIX::setsid(); exec \"sleep\", $ARGV[0]; }}' {}\n\
             nohup sleep {} >/dev/null 2>&1 &\necho started\nsleep {}",
            marks[0], marks[1], marks[2], marks[3], marks[4]
        );
        let started = Instant::now();
        let output = run(&workspace, &command, &RunOptions { timeout: Duration::from_secs(1), ..options.clone() }).await;
        assert!(output.timed_out, "{name}: {output:?}");
        assert_eq!((output.exit_code, output.signal), (None, Some(9)), "{name}");
        assert_eq!(output.stdout.text, "started\n", "{name}");
        assert!(started.elapsed() < Duration::from_secs(8), "{name}: took {:?}", started.elapsed());
        for (index, mark) in marks.iter().enumerate() {
            let left = survivors(mark).await;
            // The one case a platform may not cover: its own session, where macOS cannot refuse
            // setsid (support() says so).
            if index == 2 && !support().enforces.process_tree_killed {
                for pid in left {
                    // SAFETY: a signal to the stray `sleep` this test started.
                    unsafe { libc_kill(pid) };
                }
                continue;
            }
            assert!(left.is_empty(), "{name}: process {index} outlived the timeout: {left:?}");
        }
        assert!(output.all_processes_ended || !support().enforces.process_tree_killed, "{name}");

        // Without a timeout too: what a command leaves in the background ends with it.
        let mark = marker();
        let output = run(&workspace, &format!("sleep {mark} & echo left"), &options).await;
        assert_eq!((output.stdout.text.as_str(), output.exit_code), ("left\n", Some(0)), "{name}");
        assert!(survivors(&mark).await.is_empty(), "{name}: a background process outlived its command");
    }

    // Dropping the run (a cancelled sample) kills the command as well.
    let mark = marker();
    let dropped = tokio::time::timeout(Duration::from_millis(500), workspace.run(&format!("sleep {mark}"), &quick())).await;
    assert!(dropped.is_err());
    assert!(survivors(&mark).await.is_empty(), "the command outlived its dropped run");
}

unsafe fn libc_kill(pid: i32) {
    extern "C" {
        fn kill(pid: i32, signal: i32) -> i32;
    }
    kill(pid, 9);
}

#[tokio::test(flavor = "multi_thread")]
async fn processes_cannot_be_spawned_without_end() {
    require_sandbox();
    let workspace = workspace();
    for (name, mut options) in variants() {
        let mark = marker();
        options.timeout = Duration::from_secs(3);
        options.limits = Limits { processes: Some(40), ..Limits::default() };
        // One more process each turn, forever (the bound keeps a machine where the kernel does not
        // limit root's processes usable).
        let command = format!("n=0; while [ $n -lt 2000 ]; do sleep {mark} & n=$((n+1)); echo $n > count; done; wait");
        let started = Instant::now();
        let output = run(&workspace, &command, &options).await;
        assert!(output.timed_out, "{name}: {output:?}");
        assert!(started.elapsed() < Duration::from_secs(15), "{name}: took {:?}", started.elapsed());
        assert!(survivors(&mark).await.is_empty(), "{name}: processes outlived the command");
        assert!(output.all_processes_ended, "{name}");
        let count: u32 = std::fs::read_to_string(workspace.path().join("count")).unwrap().trim().parse().unwrap();
        if support().enforces.processes {
            // The kernel counts the user's processes, and other tests' commands come and go
            // meanwhile: well under the 2,000 the loop asks for is the point.
            assert!(count <= 150, "{name}: {count} processes started with a limit of 40");
            assert!(output.stderr.text.contains("fork") || output.stderr.text.contains("Resource"), "{name}: {}", text(&output));
        } else {
            println!("{name}: the process limit is not enforced here ({count} started): {:?}", support().notes);
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn output_is_capped() {
    require_sandbox();
    let workspace = workspace();
    for (name, mut options) in variants() {
        options.max_output_bytes = 10_000;
        let command = "head -c 3000000 /dev/zero | tr '\\0' 'a'; echo; echo end; head -c 20 /dev/zero | tr '\\0' 'e' >&2";
        let output = run(&workspace, command, &options).await;
        assert_eq!(output.exit_code, Some(0), "{name}");
        assert_eq!(output.stdout.total_bytes, 3_000_005, "{name}");
        assert!(output.stdout.truncated() && !output.stderr.truncated(), "{name}");
        assert_eq!(output.stdout.text, "a".repeat(5000), "{name}");
        assert!(output.stdout.tail.len() == 5000 && output.stdout.tail.ends_with("aaa\nend\n"), "{name}");
        assert_eq!(output.stdout.omitted_bytes, 3_000_005 - 10_000, "{name}");
        assert_eq!(output.stderr.text, "e".repeat(20), "{name}");

        // Output without end is dropped as it arrives, until the timeout.
        options.timeout = Duration::from_secs(1);
        let output = run(&workspace, "yes", &options).await;
        assert!(output.timed_out && output.stdout.total_bytes > 100_000, "{name}: {output:?}");
        assert_eq!(output.stdout.text.len() + output.stdout.tail.len(), 10_000, "{name}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn processor_time_is_limited() {
    require_sandbox();
    let workspace = workspace();
    let options = RunOptions { limits: Limits { cpu_seconds: Some(1), ..Limits::default() }, ..quick() };
    let started = Instant::now();
    let output = run(&workspace, "while :; do :; done", &options).await;
    assert!(!output.timed_out, "{output:?}");
    assert_eq!((output.exit_code, output.signal), (None, Some(24)), "{output:?}"); // SIGXCPU
    assert!(started.elapsed() < Duration::from_secs(10), "took {:?}", started.elapsed());
}

#[tokio::test(flavor = "multi_thread")]
async fn file_size_and_open_files_are_limited() {
    require_sandbox();
    let workspace = workspace();
    let limits = Limits { file_bytes: Some(1 << 20), open_files: Some(64), ..Limits::default() };
    let options = RunOptions { limits, ..quick() };
    let output = run(&workspace, "head -c 5000000 /dev/zero > big; echo rc=$?; wc -c < big", &options).await;
    assert!(output.stdout.text.starts_with("rc=1\n"), "{}", text(&output));
    assert_eq!(output.stdout.text.lines().nth(1).map(str::trim), Some("1048576"), "{}", text(&output));
    assert!(output.stderr.text.contains("File too large"), "{}", text(&output));

    let command = "ulimit -n; ulimit -n 4096 2>/dev/null; ulimit -n; \
         perl -e 'for (1..200) { open($f[$_], \"<\", \"/dev/null\") or die \"open $_: $!\\n\" } print \"opened all\\n\"'";
    let output = run(&workspace, command, &options).await;
    assert!(output.stdout.text.starts_with("64\n64\n") && !output.stdout.text.contains("opened all"), "{}", text(&output));
    assert!(output.stderr.text.contains("Too many open files"), "{}", text(&output));
}

#[tokio::test(flavor = "multi_thread")]
async fn memory_is_limited() {
    require_sandbox();
    let workspace = workspace();
    let limits = Limits { memory_bytes: Some(256 << 20), ..Limits::default() };
    let options = RunOptions { limits, ..quick() };
    let command = "perl -e '$x = \"a\" x (700 * 1024 * 1024); print \"allocated \", length($x), \"\\n\"; sleep 5; print \"kept\\n\"'";
    let started = Instant::now();
    let output = run(&workspace, command, &options).await;
    if cfg!(target_os = "linux") {
        // The kernel refuses the allocation.
        assert!(!output.stdout.text.contains("allocated"), "{}", text(&output));
        assert_ne!(output.exit_code, Some(0), "{output:?}");
    } else {
        // Checked twice a second: the command is stopped before it finishes.
        assert_eq!(output.limit, Some(LimitHit::Memory), "{output:?}");
        assert!(!output.stdout.text.contains("kept"), "{}", text(&output));
    }
    assert!(started.elapsed() < Duration::from_secs(5), "took {:?}", started.elapsed());

    // Under the limit the same program runs.
    let output = run(&workspace, "perl -e '$x = \"a\" x (50 * 1024 * 1024); print length($x), \"\\n\"'", &options).await;
    assert_eq!(output.stdout.text, "52428800\n", "{}", text(&output));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_workspace_cannot_grow_past_its_cap() {
    require_sandbox();
    let workspace = workspaces().create(&WorkspaceOptions { max_bytes: 3 << 20, ..WorkspaceOptions::default() }).unwrap();
    let started = Instant::now();
    let command = "while :; do head -c 1000000 /dev/zero >> grow; sleep 0.1; done";
    let output = run(&workspace, command, &quick()).await;
    assert_eq!(output.limit, Some(LimitHit::WorkspaceSize), "{output:?}");
    assert!(!output.timed_out && started.elapsed() < Duration::from_secs(10), "took {:?}", started.elapsed());
    // The file helpers refuse to add to a full workspace; a command may still delete.
    assert!(matches!(workspace.write("more.txt", "x"), Err(stellarator_shell::Error::WorkspaceFull { .. })));
    let output = run(&workspace, "rm grow; echo freed", &quick()).await;
    assert_eq!((output.stdout.text.as_str(), output.limit), ("freed\n", None));
    workspace.write("more.txt", "x").unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_command_signals_its_own_processes_only() {
    require_sandbox();
    let workspace = workspace();
    for (name, options) in variants() {
        // A process of the same user outside the sandbox, in a process group of its own.
        let mark = marker();
        let mut victim = {
            use std::os::unix::process::CommandExt as _;
            std::process::Command::new("sleep").arg(&mark).process_group(0).spawn().expect("sleep starts")
        };
        let pid = victim.id();
        let command = format!("kill -9 {pid}; echo one=$?; kill -9 -{pid}; echo group=$?; kill -0 {pid}; echo probe=$?");
        let output = run(&workspace, &command, &options).await;
        assert!(!output.stdout.text.contains("=0"), "{name}: {}", text(&output));
        assert!(victim.try_wait().unwrap().is_none(), "{name}: the outside process was killed");

        // Its own processes it may signal, by number and by group.
        let command = "sleep 300 & one=$!; kill -9 $one; wait $one; echo \"one=$?\"; \
                       sleep 300 & kill -TERM 0; echo not-reached";
        let output = run(&workspace, command, &options).await;
        assert_eq!(output.stdout.text, "one=137\n", "{name}: {}", text(&output));
        assert_eq!(output.signal, Some(15), "{name}: {output:?}");
        victim.kill().unwrap();
        victim.wait().unwrap();
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_command_cannot_become_root() {
    require_sandbox();
    let workspace = workspace();
    if !std::path::Path::new("/usr/bin/sudo").exists() {
        println!("no sudo on this machine");
        return;
    }
    for (name, options) in variants() {
        let output = run(&workspace, "sudo -n id -u; echo rc=$?", &options).await;
        assert!(!output.stdout.text.starts_with("0\n") || unsafe_is_root(), "{name}: {}", text(&output));
        assert!(!output.stdout.text.contains("rc=0"), "{name}: {}", text(&output));
    }
}

fn unsafe_is_root() -> bool {
    extern "C" {
        fn geteuid() -> u32;
    }
    // SAFETY: geteuid cannot fail.
    unsafe { geteuid() == 0 }
}

#[tokio::test(flavor = "multi_thread")]
async fn more_folders_can_be_made_readable() {
    require_sandbox();
    let workspace = workspace();
    let outside = Outside::new(false);
    let options = RunOptions { read_paths: vec![outside.dir.clone()], ..quick() };
    let secret = outside.secret.display();
    let output = run(&workspace, &format!("cat '{secret}'; echo more >> '{secret}'; echo rc=$?"), &options).await;
    assert_eq!(output.stdout.text, format!("{SECRET}rc=1\n"), "{}", text(&output));
    assert_eq!(std::fs::read_to_string(&outside.secret).unwrap(), SECRET);
}

#[tokio::test(flavor = "multi_thread")]
async fn commands_of_many_samples_run_at_once() {
    require_sandbox();
    let workspaces = workspaces();
    let started = Instant::now();
    let mut tasks = Vec::new();
    for index in 0..24 {
        let workspaces = workspaces.clone();
        tasks.push(tokio::spawn(async move {
            let workspace = workspaces.create(&WorkspaceOptions::default()).unwrap();
            let mut total = Duration::ZERO;
            for round in 0..5 {
                let began = Instant::now();
                let output = run(&workspace, &format!("echo {index}-{round} > f && cat f"), &quick()).await;
                total += began.elapsed();
                assert_eq!(output.stdout.text, format!("{index}-{round}\n"));
            }
            total / 5
        }));
    }
    let mut times = Vec::new();
    for task in tasks {
        times.push(task.await.unwrap());
    }
    times.sort();
    println!(
        "120 commands in 24 workspaces at once: {:?} in all; per command median {:?}, slowest {:?}",
        started.elapsed(),
        times[times.len() / 2],
        times[times.len() - 1]
    );
}

/// Not a proof: what the sandbox looks like from inside on this machine, for the test log.
#[tokio::test(flavor = "multi_thread")]
async fn report() {
    let began = denial_log_start();
    let support = support();
    println!("support: {support:#?}");
    if !support.available {
        print_denials(&began);
        return;
    }
    let workspace = workspace();
    println!("workspace: {}", workspace.path().display());
    let mut times = Vec::new();
    for _ in 0..20 {
        let began = Instant::now();
        run(&workspace, "true", &quick()).await;
        times.push(began.elapsed());
    }
    times.sort();
    println!("`true`, 20 times in a row: median {:?}, fastest {:?}, slowest {:?}", times[10], times[0], times[19]);
    let commands = [
        "id; whoami; echo \"HOME=$HOME\"; pwd; pwd -P; uname -a; hostname",
        "ls -la; ls -la ..; ls /",
        "env | sort",
        "bash --version | head -n 1; which git python3 perl node cc make tar curl",
        "git --version && git init -q repo && cd repo && echo a > a && git add a && \
         git -c user.name=t -c user.email=t@example.com commit -q -m first && git log --oneline",
        "python3 -c 'import sys, os, socket, tempfile; print(sys.version.split()[0], os.getcwd(), tempfile.gettempdir()); \
         a, b = socket.socketpair(); a.send(b\"x\"); print(b.recv(1))'",
        "python3 -c 'import multiprocessing as m; l = m.Lock(); print(\"multiprocessing lock ok\")'",
        "python3 -c 'import os; os.fork() or os.setsid(); print(\"setsid ok\", os.getsid(0) == os.getpid())'",
        "perl -e 'use POSIX; my $r = POSIX::setsid(); print \"setsid as group leader: $r $!\\n\"'",
        "echo 'int main(void){return 0;}' > t.c && cc t.c -o t && ./t && echo compiled-and-ran",
        "ps aux | head -n 5; echo; cat /proc/self/status 2>&1 | head -n 2",
        "date; ls -la /dev/null; df -h . | tail -n 1; ulimit -a",
        "echo a > one; ln one two; echo \"hard link inside: $?\"; mkfifo p; echo \"mkfifo: $?\"; ls -l",
        "echo x | tee /dev/stdout /dev/stderr > /dev/null; cat <(echo process-substitution)",
        "sw_vers 2>&1; xcode-select -p 2>&1; sysctl -n hw.ncpu kern.hostname 2>&1; nproc 2>&1",
        "tar --version | head -n 1; curl --version | head -n 1; node --version; ruby --version; java -version 2>&1 | head -n 2",
    ];
    for command in commands {
        let output = run(&workspace, command, &quick()).await;
        println!(
            "$ {command}\n[exit {:?}, signal {:?}, {:?}]\n{}{}",
            output.exit_code, output.signal, output.duration, output.stdout.text, output.stderr.text
        );
    }
    if matches!(support.mechanism, Mechanism::Landlock { .. }) {
        let options = RunOptions { show_processes: true, ..quick() };
        let output = run(&workspace, "ps aux | wc -l; head -n 3 /proc/self/status", &options).await;
        println!("with show_processes:\n{}{}", output.stdout.text, output.stderr.text);
    }
    let output = run(&workspace, "curl -sS -m 10 -o /dev/null -w '%{http_code}\\n' https://example.com", &RunOptions { network: true, ..quick() }).await;
    println!("with the network on, curl https://example.com: {}{}", output.stdout.text, output.stderr.text);
    print_denials(&began);
}

/// macOS: the time, in the form `log show --start` takes.
fn denial_log_start() -> String {
    let now = std::process::Command::new("date").arg("+%Y-%m-%d %H:%M:%S").output();
    now.map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned()).unwrap_or_default()
}

/// macOS: what Seatbelt refused since `began`, from the system log, each kind once. This is how a
/// rule missing from the profile shows.
fn print_denials(began: &str) {
    if !cfg!(target_os = "macos") {
        return;
    }
    let log = std::process::Command::new("/usr/bin/log")
        .args(["show", "--start", began, "--style", "compact", "--predicate", "sender == \"Sandbox\""])
        .output();
    let Ok(log) = log else { return };
    let text = String::from_utf8_lossy(&log.stdout);
    let mut seen = std::collections::BTreeMap::new();
    for line in text.lines() {
        // "... Sandbox: perl(1234) deny(1) file-read-data /path"
        let Some((_, denial)) = line.split_once(" deny(") else { continue };
        let program = line.split("Sandbox: ").nth(1).and_then(|s| s.split('(').next()).unwrap_or("?");
        let what = denial.split_once(") ").map_or(denial, |(_, rest)| rest);
        *seen.entry(format!("{program}: {what}")).or_insert(0u32) += 1;
    }
    println!("Seatbelt refusals in the system log since {began} ({} kinds):", seen.len());
    for (denial, count) in seen.iter().take(400) {
        println!("  {count:>4}x {denial}");
    }
}
