//! A confined program reaches what it was given and nothing else. Landlock needs a kernel that has it,
//! with its network rules, and a sandbox that lets a process use it; where those are missing the
//! subcommand refuses to run anything, which is what these tests then see. The deployed probes run
//! the same checks inside the factory's sandbox.
#![cfg(target_os = "linux")]

use std::{
    net::TcpListener,
    path::Path,
    process::{Command, Output},
};

fn confined(job: &Path, program: &str, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_veoveo-mcp-thumb"))
        .arg("confine")
        .arg(job)
        .arg("--")
        .arg(program)
        .args(args)
        .output()
        .expect("the subcommand starts")
}

/// Whether this environment can confine at all. It says why not, once, in the test's output.
fn available(job: &Path) -> bool {
    let output = confined(job, "/bin/true", &[]);
    if !output.status.success() {
        eprintln!(
            "skipped, nothing here can be confined: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    output.status.success()
}

#[test]
fn nothing_runs_when_the_limits_cannot_be_applied_or_the_arguments_are_wrong() {
    let job = tempfile::tempdir().unwrap();
    let wrong = Command::new(env!("CARGO_BIN_EXE_veoveo-mcp-thumb"))
        .arg("confine")
        .arg(job.path())
        .arg("/bin/true")
        .output()
        .unwrap();
    assert!(!wrong.status.success());
}

#[test]
fn it_writes_its_job_and_nothing_else() {
    let job = tempfile::tempdir().unwrap();
    if !available(job.path()) {
        return;
    }
    let inside = job.path().join("page.png");
    let wrote = confined(
        job.path(),
        "/bin/sh",
        &["-c", &format!("echo x > {}", inside.display())],
    );
    assert!(wrote.status.success(), "{wrote:?}");
    assert!(inside.exists());

    let elsewhere = tempfile::tempdir().unwrap();
    let outside = elsewhere.path().join("stolen");
    let refused = confined(
        job.path(),
        "/bin/sh",
        &["-c", &format!("echo x > {}", outside.display())],
    );
    assert!(!refused.status.success(), "{refused:?}");
    assert!(!outside.exists());
}

#[test]
fn it_reads_nothing_of_another_job_or_the_servers_keys() {
    let job = tempfile::tempdir().unwrap();
    if !available(job.path()) {
        return;
    }
    let elsewhere = tempfile::tempdir().unwrap();
    let secret = elsewhere.path().join("key");
    std::fs::write(&secret, "the key").unwrap();
    let refused = confined(job.path(), "/bin/cat", &[secret.to_str().unwrap()]);
    assert!(!refused.status.success(), "{refused:?}");
    assert!(!String::from_utf8_lossy(&refused.stdout).contains("the key"));
}

#[test]
fn it_opens_no_connection() {
    let job = tempfile::tempdir().unwrap();
    if !available(job.path()) {
        return;
    }
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    // A connection a shell's /dev/tcp opens, which needs no tool to be installed.
    let refused = confined(
        job.path(),
        "/bin/bash",
        &["-c", &format!("exec 3<>/dev/tcp/127.0.0.1/{port}")],
    );
    assert!(!refused.status.success(), "{refused:?}");
    listener.set_nonblocking(true).unwrap();
    assert!(listener.accept().is_err(), "the connection arrived");
}
