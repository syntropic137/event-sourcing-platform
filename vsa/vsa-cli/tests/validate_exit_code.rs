//! `vsa validate` exit code: the gate a consumer's CI actually sees.
//!
//! Each test copies `fixtures/hexagonal-rust` (a context that validates with 0
//! errors and 0 warnings) into a temp dir, mutates the copy, and runs the real
//! binary. The negative control is the one a downstream consumer relies on:
//! removing `_shared/` yields one warning, and with `max_warnings: 0` that must
//! fail the run. Before the exit code honoured `max_warnings`, it exited 0.

mod common;

use common::{set_validation_line, validate, workspace};
use predicates::str::contains;
use std::fs;

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
