//! Shared helpers for the CLI tests: copy the clean hexagonal Rust fixture
//! into a temp dir and run the real `vsa validate` on it.

#![allow(dead_code)]

use assert_cmd::Command;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

pub fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hexagonal-rust")
}

pub fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

pub fn workspace() -> TempDir {
    let dir = TempDir::new().unwrap();
    copy_dir(&fixture(), dir.path());
    dir
}

pub fn validate(dir: &Path) -> assert_cmd::assert::Assert {
    Command::cargo_bin("vsa")
        .unwrap()
        .current_dir(dir)
        .args(["validate", "-c", "vsa.yaml"])
        .assert()
}

pub fn set_validation_line(dir: &Path, from: &str, to: &str) {
    let path = dir.join("vsa.yaml");
    // A Windows checkout may have CRLF line endings; edit in LF.
    let yaml = fs::read_to_string(&path).unwrap().replace("\r\n", "\n");
    assert!(yaml.contains(from), "fixture vsa.yaml has no line {from:?}");
    fs::write(&path, yaml.replace(from, to)).unwrap();
}

/// Write `contents` to `relative` under `dir`, creating parent folders.
pub fn write(dir: &Path, relative: &str, contents: &str) {
    let path = dir.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}
