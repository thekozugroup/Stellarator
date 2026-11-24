//! The file helpers: they read, write, edit and list inside the workspace, and refuse every path
//! that leaves it. No sandbox is involved: they run in the test process, on every platform.

mod common;

use common::{workspace, workspaces, Outside, SECRET};
use stellarator_shell::{EntryKind, Error, FileLimits, WorkspaceOptions};

#[test]
fn read_write_and_list() {
    let workspace = workspace();
    workspace.write("notes.txt", "one\n").unwrap();
    workspace.write("src/deep/er/main.py", "print('hi')\n").unwrap();
    assert_eq!(workspace.read_to_string("notes.txt").unwrap(), "one\n");
    assert_eq!(workspace.read("src/deep/er/main.py").unwrap(), b"print('hi')\n");
    // The same file by other spellings: an absolute path inside, and one with `.` and `..`.
    assert_eq!(workspace.read_to_string(workspace.path().join("notes.txt")).unwrap(), "one\n");
    assert_eq!(workspace.read_to_string("./src/../notes.txt").unwrap(), "one\n");
    // Writing replaces.
    workspace.write("notes.txt", "two\n").unwrap();
    assert_eq!(std::fs::read_to_string(workspace.path().join("notes.txt")).unwrap(), "two\n");

    let listing = workspace.list(".").unwrap();
    let entries: Vec<(&str, EntryKind, u64)> = listing.entries.iter().map(|e| (e.name.as_str(), e.kind, e.size)).collect();
    assert_eq!(entries, [("notes.txt", EntryKind::File, 4), ("src", EntryKind::Dir, 0)]);
    assert!(!listing.truncated);
    assert_eq!(workspace.list("").unwrap(), listing);
    assert_eq!(workspace.list("src/deep/er").unwrap().entries[0].name, "main.py");

    assert!(matches!(workspace.read("missing.txt"), Err(Error::Io { .. })));
    assert!(matches!(workspace.read("src"), Err(Error::WrongKind { .. })));
    assert!(matches!(workspace.list("notes.txt"), Err(Error::WrongKind { .. })));
    assert!(matches!(workspace.write("src", "x"), Err(Error::WrongKind { .. })));
    assert!(matches!(workspace.read(""), Err(Error::InvalidPath(_))));
    assert!(matches!(workspace.read("a\0b"), Err(Error::InvalidPath(_))));

    workspace.write("binary.bin", [0xff, 0xfe, 0x00]).unwrap();
    assert_eq!(workspace.read("binary.bin").unwrap(), [0xff, 0xfe, 0x00]);
    assert!(matches!(workspace.read_to_string("binary.bin"), Err(Error::NotText { .. })));
}

#[test]
fn edit_replaces_one_exact_match() {
    let workspace = workspace();
    workspace.write("code.rs", "fn a() {}\nfn b() {}\nfn a_again() {}\n").unwrap();
    workspace.edit("code.rs", "fn b() {}", "fn b() { 1 }").unwrap();
    assert_eq!(workspace.read_to_string("code.rs").unwrap(), "fn a() {}\nfn b() { 1 }\nfn a_again() {}\n");

    assert!(matches!(workspace.edit("code.rs", "fn c()", "x"), Err(Error::NoMatch { .. })));
    assert!(matches!(workspace.edit("code.rs", "", "x"), Err(Error::NoMatch { .. })));
    match workspace.edit("code.rs", "fn a", "fn z") {
        Err(Error::MultipleMatches { count, .. }) => assert_eq!(count, 2),
        other => panic!("expected MultipleMatches, got {other:?}"),
    }
    // A refused edit changes nothing.
    assert_eq!(workspace.read_to_string("code.rs").unwrap(), "fn a() {}\nfn b() { 1 }\nfn a_again() {}\n");
    assert!(matches!(workspace.edit("missing.rs", "a", "b"), Err(Error::Io { .. })));
}

#[test]
fn paths_that_leave_the_workspace_are_refused() {
    let workspace = workspace();
    let outside = Outside::new(false);
    workspace.write("inside.txt", "inside").unwrap();
    let home_file = workspace.home().join("h.txt");
    std::fs::write(&home_file, "home").unwrap();

    let escapes = [
        "..".to_string(),
        "../home/h.txt".to_string(),
        "../../x".to_string(),
        "a/../../home/h.txt".to_string(),
        "./../workspace/../home/h.txt".to_string(),
        home_file.display().to_string(),
        outside.secret.display().to_string(),
        format!("{}/../home/h.txt", workspace.path().display()),
    ];
    for path in &escapes {
        assert!(matches!(workspace.read(path), Err(Error::OutsideWorkspace { .. })), "read {path}");
        assert!(matches!(workspace.write(path, "hacked"), Err(Error::OutsideWorkspace { .. })), "write {path}");
        assert!(matches!(workspace.edit(path, "home", "hacked"), Err(Error::OutsideWorkspace { .. })), "edit {path}");
        assert!(matches!(workspace.list(path), Err(Error::OutsideWorkspace { .. })), "list {path}");
    }
    assert_eq!(std::fs::read_to_string(&home_file).unwrap(), "home");
    assert_eq!(std::fs::read_to_string(&outside.secret).unwrap(), SECRET);
    assert_eq!(outside.names(), ["secret.txt"]);
    // `..` that stays inside is a path like any other.
    workspace.write("a/b/c.txt", "c").unwrap();
    assert_eq!(workspace.read_to_string("a/b/../../inside.txt").unwrap(), "inside");
}

#[cfg(unix)]
#[test]
fn symlinks_that_leave_the_workspace_are_refused() {
    use std::os::unix::fs::symlink;
    let workspace = workspace();
    let outside = Outside::new(false);
    let root = workspace.path();
    workspace.write("real.txt", "real").unwrap();
    workspace.write("dir/file.txt", "in dir").unwrap();

    // What a command could plant: links to a file, to a folder, to the parent, and a chain.
    symlink(&outside.secret, root.join("to-file")).unwrap();
    symlink(&outside.dir, root.join("to-dir")).unwrap();
    symlink("..", root.join("up")).unwrap();
    symlink("../..", root.join("dir/up2")).unwrap();
    symlink("to-dir", root.join("chain")).unwrap();
    symlink(outside.dir.join("not-there.txt"), root.join("dangling")).unwrap();
    symlink("/", root.join("slash")).unwrap();

    let escapes = [
        "to-file",
        "to-dir/secret.txt",
        "to-dir/new.txt",
        "up/home/x.txt",
        "dir/up2/home/x.txt",
        "chain/secret.txt",
        "dangling",
        "slash/etc/hosts",
    ];
    for path in escapes {
        assert!(matches!(workspace.read(path), Err(Error::OutsideWorkspace { .. })), "read {path}: {:?}", workspace.read(path));
        assert!(matches!(workspace.write(path, "hacked"), Err(Error::OutsideWorkspace { .. })), "write {path}");
        assert!(matches!(workspace.edit(path, "T", "hacked"), Err(Error::OutsideWorkspace { .. })), "edit {path}");
    }
    for folder in ["to-dir", "up", "chain", "slash", "dir/up2"] {
        assert!(matches!(workspace.list(folder), Err(Error::OutsideWorkspace { .. })), "list {folder}");
    }
    assert_eq!(std::fs::read_to_string(&outside.secret).unwrap(), SECRET);
    assert_eq!(outside.names(), ["secret.txt"]);
    assert!(!workspace.home().join("x.txt").exists());

    // Links that stay inside work, and a listing shows a link as a link.
    symlink("real.txt", root.join("alias")).unwrap();
    symlink("dir", root.join("dir-alias")).unwrap();
    assert_eq!(workspace.read_to_string("alias").unwrap(), "real");
    workspace.write("alias", "through the link").unwrap();
    assert_eq!(workspace.read_to_string("real.txt").unwrap(), "through the link");
    assert_eq!(workspace.read_to_string("dir-alias/file.txt").unwrap(), "in dir");
    let listing = workspace.list(".").unwrap();
    let kind = |name: &str| listing.entries.iter().find(|e| e.name == name).map(|e| e.kind);
    assert_eq!((kind("alias"), kind("to-dir"), kind("dir")), (Some(EntryKind::Symlink), Some(EntryKind::Symlink), Some(EntryKind::Dir)));
}

/// A named pipe where a file is expected must not make the helper wait for a writer.
#[cfg(unix)]
#[test]
fn a_named_pipe_is_not_a_file() {
    let workspace = workspace();
    let made = std::process::Command::new("mkfifo").arg(workspace.path().join("pipe")).status().unwrap();
    assert!(made.success());
    let started = std::time::Instant::now();
    assert!(matches!(workspace.read("pipe"), Err(Error::WrongKind { .. })), "{:?}", workspace.read("pipe"));
    assert!(workspace.write("pipe", "x").is_err());
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
    assert_eq!(workspace.list(".").unwrap().entries[0].kind, EntryKind::Other);
}

#[test]
fn sizes_are_capped() {
    let limits = FileLimits { max_read_bytes: 100, max_write_bytes: 50, max_list_entries: 3 };
    let options = WorkspaceOptions { max_bytes: 64 * 1024, files: limits, ..WorkspaceOptions::default() };
    let workspace = workspaces().create(&options).unwrap();

    assert!(matches!(workspace.write("big.txt", "x".repeat(51)), Err(Error::TooLarge { size: 51, limit: 50, .. })));
    assert!(!workspace.path().join("big.txt").exists());
    workspace.write("ok.txt", "x".repeat(50)).unwrap();

    // A file a command wrote, larger than the helper reads.
    std::fs::write(workspace.path().join("large.txt"), "y".repeat(101)).unwrap();
    assert!(matches!(workspace.read("large.txt"), Err(Error::TooLarge { size: 101, limit: 100, .. })));
    assert!(matches!(workspace.edit("large.txt", "y", "z"), Err(Error::TooLarge { .. })));
    // An edit may not grow a file past the write cap.
    assert!(matches!(workspace.edit("ok.txt", &"x".repeat(50), &"z".repeat(60)), Err(Error::TooLarge { .. })));

    for name in ["a", "b", "c", "d"] {
        workspace.write(name, "1").unwrap();
    }
    let listing = workspace.list(".").unwrap();
    assert_eq!((listing.entries.len(), listing.truncated), (3, true));

    // The workspace's own cap: a command filled it, the helper will not add to it.
    std::fs::write(workspace.path().join("fill.bin"), vec![1u8; 80 * 1024]).unwrap();
    assert!(workspace.measure() >= 80 * 1024); // as `run` does after every command
    match workspace.write("more.txt", "x") {
        Err(Error::WorkspaceFull { limit, .. }) => assert_eq!(limit, 64 * 1024),
        other => panic!("expected WorkspaceFull, got {other:?}"),
    }
}
