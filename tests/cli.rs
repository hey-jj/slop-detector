//! CLI behavior tests: version flag, the input cap, and pipe handling.

use std::io::{Read, Write};
use std::process::{Command, Stdio};

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_slop-detector"))
}

#[test]
fn version_flag_prints_the_crate_version() {
    for flag in ["-V", "--version"] {
        let out = bin().arg(flag).output().unwrap();
        assert!(out.status.success(), "{flag}");
        let stdout = String::from_utf8(out.stdout).unwrap();
        assert_eq!(
            stdout,
            format!("slop-detector {}\n", env!("CARGO_PKG_VERSION")),
            "{flag}"
        );
    }
}

#[test]
fn help_names_the_cap_and_the_version_flag() {
    let out = bin().arg("--help").output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("4 MiB"));
    assert!(stdout.contains("--version"));
}

#[test]
fn help_names_the_allow_term_flag_and_bundle_mode() {
    let out = bin().arg("--help").output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("--allow-term"));
    assert!(stdout.contains("bundle report"));
}

#[test]
fn single_file_output_is_the_analyze_report_verbatim() {
    let dir = std::env::temp_dir().join("slop-detector-cli-single");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("one.txt");
    let text = "We delve into the numbers, not the narrative.";
    std::fs::write(&path, text).unwrap();
    let out = bin().arg(&path).output().unwrap();
    std::fs::remove_file(&path).ok();
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    let expected = serde_json::to_string_pretty(&slop_detector::analyze(text)).unwrap() + "\n";
    assert_eq!(
        stdout, expected,
        "single-file output must stay byte-identical to analyze()"
    );
    assert!(
        !stdout.contains("\"files\""),
        "no bundle wrapper on one input"
    );
}

#[test]
fn two_files_produce_the_bundle_report() {
    let dir = std::env::temp_dir().join("slop-detector-cli-bundle");
    std::fs::create_dir_all(&dir).unwrap();
    let shared = "Our platform unifies ingestion, storage, and search behind one \
                  binding contract for every downstream team.";
    let a = dir.join("a.txt");
    let b = dir.join("b.txt");
    std::fs::write(&a, format!("Variant one.\n\n{shared}\n")).unwrap();
    std::fs::write(&b, format!("Variant two.\n\n{shared}\n")).unwrap();
    let out = bin().arg(&a).arg(&b).output().unwrap();
    std::fs::remove_file(&a).ok();
    std::fs::remove_file(&b).ok();
    assert!(out.status.success(), "{:?}", out.status);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["files"].as_array().unwrap().len(), 2);
    let cross = v["cross_file_duplication"].as_array().unwrap();
    assert_eq!(cross.len(), 1, "{v}");
    assert_eq!(cross[0]["occurrences"].as_array().unwrap().len(), 2);
}

#[test]
fn allow_term_flag_labels_matching_findings() {
    let mut child = bin()
        .arg("--allow-term")
        .arg("delve")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"We delve into the numbers.")
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let hit = &v["quality_patterns"][0];
    assert_eq!(hit["rule_id"], "SLOP-A001");
    assert_eq!(hit["topic_term"], true, "{v}");
}

#[test]
fn over_cap_file_is_rejected_with_exit_40() {
    let dir = std::env::temp_dir().join("slop-detector-cli-test");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("over-cap.txt");
    // One byte over the 4 MiB cap.
    let mut f = std::fs::File::create(&path).unwrap();
    let chunk = vec![b'a'; 1024 * 1024];
    for _ in 0..4 {
        f.write_all(&chunk).unwrap();
    }
    f.write_all(b"x").unwrap();
    drop(f);

    let out = bin().arg(&path).output().unwrap();
    std::fs::remove_file(&path).ok();
    assert_eq!(out.status.code(), Some(40), "{:?}", out.status);
    assert!(out.stdout.is_empty());
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("limit"), "{stderr}");
}

#[test]
fn over_cap_stdin_is_rejected_with_exit_40() {
    let mut child = bin()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let chunk = vec![b'a'; 1024 * 1024];
    for _ in 0..4 {
        stdin.write_all(&chunk).unwrap();
    }
    stdin.write_all(b"xx").unwrap();
    drop(stdin);
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(40), "{:?}", out.status);
}

#[test]
fn at_cap_input_is_accepted() {
    let mut child = bin()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let chunk = vec![b'a'; 1024 * 1024];
    for _ in 0..4 {
        stdin.write_all(&chunk).unwrap();
    }
    drop(stdin);
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "{:?}", out.status);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("\"stats\""));
}

#[test]
fn closed_output_pipe_exits_quietly() {
    // Match-dense input produces a report far larger than the pipe buffer.
    // Closing the read end mid-write must end the run with exit 0 and no
    // panic, the standard CLI pipe convention.
    let mut child = bin()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let input = "very ".repeat(60_000);
    stdin.write_all(input.as_bytes()).unwrap();
    drop(stdin);
    // Read a few bytes so the child starts writing, then close the pipe.
    let mut stdout = child.stdout.take().unwrap();
    let mut first = [0u8; 16];
    let _ = stdout.read(&mut first);
    drop(stdout);
    let status = child.wait().unwrap();
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .ok();
    assert!(status.success(), "status {status:?}, stderr: {stderr}");
    assert!(!stderr.contains("panicked"), "{stderr}");
}
