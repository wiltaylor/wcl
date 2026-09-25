//! A closed stdout or stderr is not an error. `wcl parse big.wcl | head -1`
//! used to panic with "failed printing to stdout: Broken pipe" and exit
//! 101; now `wcl` stops quietly and exits 0, on every platform.
//!
//! Each test spawns `wcl` with its output on a pipe, reads a few bytes,
//! and closes the pipe while `wcl` still has more than a pipe buffer
//! (64 KiB) left to write. Plain `std::process` pipes rather than a shell,
//! so the tests run on Windows too.

use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};

use tempfile::TempDir;

const LINES: usize = 3000;

fn wcl() -> Command {
    Command::new(env!("CARGO_BIN_EXE_wcl"))
}

/// Top-level fields, one a line: `parse` and `fmt` echo each back, well
/// past a pipe buffer.
fn fields(dir: &Path, name: &str, value: &str) -> std::path::PathBuf {
    let src: String = (0..LINES)
        .map(|i| format!("@schemaless field_{i} = \"{value} {i}, padded so each line is long\"\n"))
        .collect();
    let path = dir.join(name);
    std::fs::write(&path, src).expect("write fixture");
    path
}

/// Blocks no schema declares: each is a violation, so `check` has a
/// diagnostic per line to write.
fn undeclared_blocks(dir: &Path) -> std::path::PathBuf {
    let src: String = (0..LINES).map(|i| format!("thing t{i} {{}}\n")).collect();
    let path = dir.join("blocks.wcl");
    std::fs::write(&path, src).expect("write fixture");
    path
}

/// Read a few bytes of `child`'s stdout, close it, and wait. Returns the
/// exit code and everything written to stderr.
fn close_stdout_early(mut child: Child) -> (Option<i32>, String) {
    let mut stdout = child.stdout.take().expect("stdout piped");
    let mut head = [0u8; 16];
    stdout.read_exact(&mut head).expect("read the first bytes");
    drop(stdout);
    let out = child.wait_with_output().expect("wait for wcl");
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn spawn_on_stdout(cmd: &mut Command) -> Child {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn wcl")
}

/// The contract every test checks: exit 0, and no panic on stderr.
fn assert_quiet_stop((code, stderr): (Option<i32>, String)) {
    assert!(!stderr.contains("panicked"), "wcl panicked:\n{stderr}");
    assert_eq!(code, Some(0), "stderr:\n{stderr}");
}

#[test]
fn parse_stops_quietly_when_stdout_closes() {
    let tmp = TempDir::new().expect("tempdir");
    let file = fields(tmp.path(), "a.wcl", "value");
    let child = spawn_on_stdout(wcl().arg("parse").arg(&file));
    assert_quiet_stop(close_stdout_early(child));
}

#[test]
fn fmt_stops_quietly_when_stdout_closes() {
    let tmp = TempDir::new().expect("tempdir");
    let file = fields(tmp.path(), "a.wcl", "value");
    let child = spawn_on_stdout(wcl().arg("fmt").arg(&file));
    assert_quiet_stop(close_stdout_early(child));
}

#[test]
fn check_json_stops_quietly_when_stdout_closes() {
    // The document fails its schema (exit 2 when read in full), but the
    // reader left first: a closed stream outranks the verdict.
    let tmp = TempDir::new().expect("tempdir");
    let file = undeclared_blocks(tmp.path());
    let child = spawn_on_stdout(wcl().args(["check", "--json"]).arg(&file));
    assert_quiet_stop(close_stdout_early(child));
}

#[test]
fn eval_stops_quietly_when_stdout_closes() {
    let tmp = TempDir::new().expect("tempdir");
    let items: String = (0..LINES)
        .map(|i| format!("  \"item {i}, padded so the value is long\",\n"))
        .collect();
    let file = tmp.path().join("list.wcl");
    std::fs::write(&file, format!("@schemaless big = [\n{items}]\n")).expect("write fixture");
    let child = spawn_on_stdout(wcl().arg("eval").arg(&file).arg("big"));
    assert_quiet_stop(close_stdout_early(child));
}

#[test]
fn diff_stops_quietly_when_stdout_closes() {
    let tmp = TempDir::new().expect("tempdir");
    let old = fields(tmp.path(), "old.wcl", "value");
    let new = fields(tmp.path(), "new.wcl", "changed");
    let child = spawn_on_stdout(wcl().arg("diff").arg(&old).arg(&new));
    assert_quiet_stop(close_stdout_early(child));
}

#[test]
fn wdoc_build_stops_quietly_when_stdout_closes() {
    // A build's stdout is one line, so close it before `wcl` writes it:
    // the build takes far longer than the drop.
    let tmp = TempDir::new().expect("tempdir");
    let src = tmp.path().join("doc.wcl");
    std::fs::write(&src, "import <wdoc.wcl>\npage index { h1 \"Home\" }\n").expect("write doc");
    let mut child = spawn_on_stdout(
        wcl()
            .args(["wdoc", "build"])
            .arg(&src)
            .arg("--out")
            .arg(tmp.path().join("out")),
    );
    drop(child.stdout.take());
    let out = child.wait_with_output().expect("wait for wcl");
    assert_quiet_stop((
        out.status.code(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    ));
    assert!(tmp.path().join("out").join("index.html").exists());
}

#[test]
fn a_failing_check_stops_quietly_when_stderr_closes() {
    // The diagnostics go to stderr; the reader closes it partway through.
    // With stderr gone a panic could not be seen, but it would still exit
    // 101, so the exit code is the witness.
    let tmp = TempDir::new().expect("tempdir");
    let file = undeclared_blocks(tmp.path());
    let mut child = wcl()
        .arg("check")
        .arg(&file)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn wcl");
    let mut stderr = child.stderr.take().expect("stderr piped");
    let mut head = [0u8; 16];
    stderr.read_exact(&mut head).expect("read the first bytes");
    drop(stderr);
    let status = child.wait().expect("wait for wcl");
    assert_eq!(status.code(), Some(0));
}

#[test]
fn a_failing_check_still_exits_2_when_stderr_is_read() {
    // The control for the test above: read in full, the verdict stands.
    let tmp = TempDir::new().expect("tempdir");
    let file = undeclared_blocks(tmp.path());
    let out = wcl()
        .arg("check")
        .arg(&file)
        .stdin(Stdio::null())
        .output()
        .expect("run wcl");
    assert_eq!(out.status.code(), Some(2));
}
