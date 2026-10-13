//! `vsa validate` exit code: the gate a consumer's CI actually sees.
//!
//! Each test copies `fixtures/hexagonal-rust` (a context that validates with 0
//! errors and 0 warnings) into a temp dir, mutates the copy, and runs the real
//! binary. The negative control is the one a downstream consumer relies on:
//! removing `_shared/` yields one warning, and with `max_warnings: 0` that must
//! fail the run. Before the exit code honoured `max_warnings`, it exited 0.

use assert_cmd::Command;
use predicates::str::contains;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hexagonal-rust")
}

fn copy_dir(from: &Path, to: &Path) {
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

fn workspace() -> TempDir {
    let dir = TempDir::new().unwrap();
    copy_dir(&fixture(), dir.path());
    dir
}

fn validate(dir: &Path) -> assert_cmd::assert::Assert {
    Command::cargo_bin("vsa")
        .unwrap()
        .current_dir(dir)
        .args(["validate", "-c", "vsa.yaml"])
        .assert()
}

fn set_validation_line(dir: &Path, from: &str, to: &str) {
    let path = dir.join("vsa.yaml");
    let yaml = fs::read_to_string(&path).unwrap();
    assert!(yaml.contains(from), "fixture vsa.yaml has no line {from:?}");
    fs::write(&path, yaml.replace(from, to)).unwrap();
}

#[test]
fn clean_fixture_passes() {
    let dir = workspace();
    validate(dir.path()).success().stdout(contains("All checks passed"));
}

#[test]
fn removing_shared_fails_under_max_warnings_zero() {
    let dir = workspace();
    fs::remove_dir_all(dir.path().join("contexts/orders/_shared")).unwrap();
    validate(dir.path())
        .failure()
        .stdout(contains("missing _shared folder"))
        .stderr(contains("1 warning(s) (max_warnings = 0)"));
}

#[test]
fn warnings_within_budget_pass() {
    let dir = workspace();
    set_validation_line(dir.path(), "  max_warnings: 0", "  max_warnings: 1");
    fs::remove_dir_all(dir.path().join("contexts/orders/_shared")).unwrap();
    validate(dir.path()).success().stdout(contains("passed with warnings"));
}

#[test]
fn omitted_budget_reports_warnings_without_failing() {
    let dir = workspace();
    set_validation_line(dir.path(), "  max_warnings: 0\n", "");
    fs::remove_dir_all(dir.path().join("contexts/orders/_shared")).unwrap();
    validate(dir.path()).success().stdout(contains("missing _shared folder"));
}

#[test]
fn errors_fail_the_run() {
    let dir = workspace();
    // A domain file importing infrastructure breaks dependency direction (VSA027).
    let aggregate = dir.path().join("contexts/orders/domain/aggregate_order/order_aggregate.rs");
    let source = fs::read_to_string(&aggregate).unwrap();
    fs::write(&aggregate, format!("use crate::infrastructure;\n{source}")).unwrap();
    validate(dir.path()).failure().stderr(contains("1 error(s)"));
}

#[test]
fn non_blocking_errors_pass_but_the_warning_budget_still_binds() {
    let dir = workspace();
    set_validation_line(dir.path(), "  fail_on_errors: true", "  fail_on_errors: false");
    let aggregate = dir.path().join("contexts/orders/domain/aggregate_order/order_aggregate.rs");
    let source = fs::read_to_string(&aggregate).unwrap();
    fs::write(&aggregate, format!("use crate::infrastructure;\n{source}")).unwrap();
    validate(dir.path()).success().stdout(contains("errors are non-blocking"));

    fs::remove_dir_all(dir.path().join("contexts/orders/_shared")).unwrap();
    validate(dir.path()).failure().stderr(contains("1 warning(s) (max_warnings = 0)"));
}
