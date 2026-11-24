//! Workspaces: where they are, how they start, and that they are deleted. Every platform.

mod common;

use common::OwnBase;
use stellarator_shell::{support, Error, RunOptions, WorkspaceOptions, Workspaces};

#[test]
fn the_path_is_short_and_has_the_same_shape_every_time() {
    let own = OwnBase::new("shape");
    let workspaces = Workspaces::open(&own.0).unwrap();
    let first = workspaces.create(&WorkspaceOptions::default()).unwrap();
    let second = workspaces.create(&WorkspaceOptions::default()).unwrap();

    // <base>/<n>/workspace, with the lowest free numbers.
    assert_eq!(first.path(), workspaces.base().join("1").join("workspace"));
    assert_eq!(second.path(), workspaces.base().join("2").join("workspace"));
    assert_eq!((first.slot(), second.slot()), (1, 2));
    assert_eq!(first.home(), workspaces.base().join("1").join("home"));
    assert_eq!(first.tmp(), workspaces.base().join("1").join("tmp"));
    // The real path: no symlink in it, so every tool prints the same one.
    assert_eq!(std::fs::canonicalize(first.path()).unwrap().to_string_lossy().trim_start_matches(r"\\?\"), first.path().to_string_lossy());
    for folder in [first.path(), first.home(), first.tmp()] {
        assert!(folder.is_dir() && std::fs::read_dir(folder).unwrap().next().is_none());
    }

    // A freed number is used again.
    let path = first.path().to_path_buf();
    first.close().unwrap();
    assert!(!path.exists() && !workspaces.base().join("1").exists());
    let third = workspaces.create(&WorkspaceOptions::default()).unwrap();
    assert_eq!(third.path(), path);
}

/// The default folder holds no user name and is the one the documentation names.
#[test]
fn the_default_folder_is_fixed_per_platform() {
    let base = Workspaces::default_base();
    let text = base.to_string_lossy().into_owned();
    if cfg!(target_os = "macos") {
        assert_eq!(text, "/private/tmp/stellarator");
    } else if cfg!(unix) {
        assert_eq!(text, "/var/tmp/stellarator");
    } else {
        assert!(text.ends_with(r"Stellarator\workspaces"), "{text}");
    }
    for name in ["USER", "USERNAME", "LOGNAME"] {
        if let Ok(user) = std::env::var(name) {
            // "runner" or "root" would have to be a path component to be the user's name.
            assert!(!base.components().any(|c| c.as_os_str().to_string_lossy() == user), "{text} has the user name {user}");
        }
    }
    assert!(matches!(Workspaces::open("relative/path"), Err(Error::Base { .. })));
}

#[test]
fn dropping_a_workspace_deletes_it() {
    let own = OwnBase::new("drop");
    let workspaces = Workspaces::open(&own.0).unwrap();
    let workspace = workspaces.create(&WorkspaceOptions::default()).unwrap();
    let slot = workspaces.base().join(workspace.slot().to_string());
    workspace.write("a/b/c.txt", "c").unwrap();
    std::fs::write(workspace.home().join(".cache"), "cache").unwrap();
    #[cfg(unix)]
    {
        // What a command can leave: a folder nobody may enter, and a link out of the workspace.
        use std::os::unix::fs::PermissionsExt as _;
        let locked = workspace.path().join("locked");
        std::fs::create_dir(&locked).unwrap();
        std::fs::write(locked.join("inner.txt"), "x").unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        std::os::unix::fs::symlink(own.0.parent().unwrap(), workspace.path().join("out")).unwrap();
    }
    drop(workspace);
    assert!(!slot.exists(), "{} is still there", slot.display());
    // The link was deleted, not followed.
    assert!(own.0.parent().unwrap().is_dir());
}

#[test]
fn workspaces_left_by_a_dead_process_are_swept() {
    let own = OwnBase::new("sweep");
    let workspaces = Workspaces::open(&own.0).unwrap();
    let live = workspaces.create(&WorkspaceOptions::default()).unwrap();
    live.write("keep.txt", "keep").unwrap();
    // As a process that died would leave them: folders without a held lock.
    for stale in ["2", "7"] {
        std::fs::create_dir_all(workspaces.base().join(stale).join("workspace")).unwrap();
        std::fs::write(workspaces.base().join(stale).join("workspace").join("old.txt"), "old").unwrap();
    }
    // Not a workspace: left alone.
    std::fs::create_dir(workspaces.base().join("notes")).unwrap();

    let reopened = Workspaces::open(&own.0).unwrap();
    assert!(!reopened.base().join("2").exists() && !reopened.base().join("7").exists());
    assert!(reopened.base().join("notes").is_dir());
    // The one in use stays, and its number is not handed out.
    assert_eq!(live.read_to_string("keep.txt").unwrap(), "keep");
    let next = reopened.create(&WorkspaceOptions::default()).unwrap();
    assert_eq!(next.slot(), 2);
    assert_eq!(reopened.sweep(), 0);
}

#[test]
fn workspaces_made_at_once_are_distinct() {
    let own = OwnBase::new("many");
    let workspaces = Workspaces::open(&own.0).unwrap();
    let threads: Vec<_> = (0..16)
        .map(|_| {
            let workspaces = workspaces.clone();
            std::thread::spawn(move || workspaces.create(&WorkspaceOptions::default()).unwrap())
        })
        .collect();
    let made: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    let mut slots: Vec<u32> = made.iter().map(|w| w.slot()).collect();
    slots.sort();
    assert_eq!(slots, (1..=16).collect::<Vec<u32>>());
}

#[test]
fn a_workspace_can_start_from_a_template() {
    let own = OwnBase::new("template");
    let template = own.0.with_extension("src");
    let _ = std::fs::remove_dir_all(&template);
    std::fs::create_dir_all(template.join("src/nested")).unwrap();
    std::fs::write(template.join("README.md"), "# Project\n").unwrap();
    std::fs::write(template.join("src/nested/lib.rs"), "pub fn f() {}\n").unwrap();
    std::fs::write(template.join("run.sh"), "#!/bin/sh\necho ran\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(template.join("run.sh"), std::fs::Permissions::from_mode(0o755)).unwrap();
        std::os::unix::fs::symlink("README.md", template.join("link")).unwrap();
    }

    let workspaces = Workspaces::open(&own.0).unwrap();
    let options = WorkspaceOptions { template: Some(template.clone()), ..WorkspaceOptions::default() };
    let workspace = workspaces.create(&options).unwrap();
    assert_eq!(workspace.read_to_string("README.md").unwrap(), "# Project\n");
    assert_eq!(workspace.read_to_string("src/nested/lib.rs").unwrap(), "pub fn f() {}\n");
    #[cfg(unix)]
    {
        assert_eq!(common::mode(&workspace.path().join("run.sh")), 0o755);
        assert_eq!(std::fs::read_link(workspace.path().join("link")).unwrap().to_string_lossy(), "README.md");
    }
    // A copy: the template is untouched by what happens in the workspace.
    workspace.write("README.md", "changed").unwrap();
    assert_eq!(std::fs::read_to_string(template.join("README.md")).unwrap(), "# Project\n");
    assert!(workspace.measure() > 0);

    // A template over the cap is refused, and nothing of it stays.
    std::fs::write(template.join("big.bin"), vec![0u8; 300_000]).unwrap();
    let small = WorkspaceOptions { template: Some(template.clone()), max_bytes: 100_000, ..WorkspaceOptions::default() };
    assert!(matches!(workspaces.create(&small), Err(Error::Template(_))));
    assert!(!workspaces.base().join("2").exists());
    let missing = WorkspaceOptions { template: Some(template.join("not-there")), ..WorkspaceOptions::default() };
    assert!(matches!(workspaces.create(&missing), Err(Error::Template(_))));
    let _ = std::fs::remove_dir_all(&template);
}

/// Without a sandbox a command is an error that says why; it never runs unconfined.
#[tokio::test]
async fn without_a_sandbox_commands_are_refused() {
    let support = support();
    println!("{support:#?}");
    if support.available {
        assert!(support.reason.is_none());
        return;
    }
    let reason = support.reason.clone().expect("a reason when there is no sandbox");
    assert!(!reason.is_empty());
    let own = OwnBase::new("refuse");
    let workspace = Workspaces::open(&own.0).unwrap().create(&WorkspaceOptions::default()).unwrap();
    let marker = workspace.path().join("ran.txt");
    match workspace.run("echo ran > ran.txt", &RunOptions::default()).await {
        Err(Error::Unsupported { reason: given }) => assert_eq!(given, reason),
        other => panic!("expected Unsupported, got {other:?}"),
    }
    assert!(!marker.exists(), "the command ran without a sandbox");
    // The file helpers do not need the sandbox.
    workspace.write("note.txt", "still works").unwrap();
    assert_eq!(workspace.read_to_string("note.txt").unwrap(), "still works");
}
