//! `tasks/run` turns its argument into paths under `tasks/`, so a name that
//! could leave that directory must be refused before anything is read,
//! built, or written. Each run puts a stand-in `cargo` first on `PATH`, so
//! the script never builds anything and the test can see whether it got
//! that far.
#![cfg(unix)]

use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output};

/// What the stand-in `cargo` prints, so a test can tell it ran.
const STAND_IN: &str = "stand-in cargo ran";

/// Runs `tasks/run <name>` with the stand-in `cargo`, in a directory of its
/// own so parallel tests do not share one.
fn run(name: &str, scratch: &str) -> io::Result<Output> {
    let bin = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(scratch);
    std::fs::create_dir_all(&bin)?;
    let cargo = bin.join("cargo");
    std::fs::write(&cargo, format!("#!/bin/sh\necho '{STAND_IN}'\n"))?;
    std::fs::set_permissions(&cargo, std::fs::Permissions::from_mode(0o755))?;
    Command::new("bash")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tasks/run"))
        .arg(name)
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
        .env("TINYCOMPUTER_BROWSER_EXECUTABLE", "/nonexistent/chrome")
        // A module named here is not built, so the stand-in `cargo` is only
        // ever asked to run the runner.
        .env("TINYCOMPUTER_MODULE", "/nonexistent/libtinycomputer")
        .env_remove("TASK_OUT")
        .output()
}

#[test]
fn refuses_a_task_name_that_could_leave_the_tasks_directory() -> io::Result<()> {
    for name in [
        "../../etc",
        "kashmir/../x",
        "..",
        ".",
        "a/b",
        "/tmp",
        "kash mir",
    ] {
        let output = run(name, "refuses")?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "{name:?} was accepted");
        assert!(stderr.contains("invalid task name"), "{name:?}: {stderr}");
        assert!(
            !String::from_utf8_lossy(&output.stdout).contains(STAND_IN),
            "{name:?} reached cargo"
        );
    }
    Ok(())
}

#[test]
fn refuses_an_empty_task_name() -> io::Result<()> {
    let output = run("", "empty")?;
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).contains(STAND_IN));
    Ok(())
}

#[test]
fn runs_a_plain_task_name() -> io::Result<()> {
    let output = run("kashmir_2-b", "accepts")?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains(STAND_IN));
    Ok(())
}
