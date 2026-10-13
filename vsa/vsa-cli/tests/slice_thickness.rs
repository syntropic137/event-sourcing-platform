//! Slice thickness and slice reach on a hexagonal Rust project, through the
//! real `vsa validate`. Each probe adds one slice to the clean fixture; on
//! v0.18.0 every probe below exited 0.

mod common;

use common::{validate, workspace, write};
use predicates::str::contains;

const SLICES: &str = "contexts/orders/slices";

#[test]
fn command_slice_that_decides_on_its_own_fails() {
    let dir = workspace();
    write(dir.path(), &format!("{SLICES}/rename/slice.yaml"), "name: rename\ntype: command\n");
    write(
        dir.path(),
        &format!("{SLICES}/rename/internal/rename_handler.rs"),
        "pub fn rename(title: &str, existing: &[String]) -> Result<String, String> {\n\
         if title.is_empty() { return Err(\"empty\".into()); }\n\
         if existing.iter().any(|t| t == title) { return Err(\"duplicate\".into()); }\n\
         Ok(title.to_string())\n}\n",
    );
    write(
        dir.path(),
        &format!("{SLICES}/rename/internal/rename_handler_test.rs"),
        "#[test]\nfn t() {}\n",
    );
    validate(dir.path())
        .failure()
        .stdout(contains("Command slice 'rename' never imports the domain layer"))
        .stdout(contains("Command slice 'rename' never imports a port"));
}

#[test]
fn nested_thin_command_slice_passes() {
    let dir = workspace();
    write(
        dir.path(),
        &format!("{SLICES}/cancel_order/slice.yaml"),
        "name: cancel_order\ntype: command\n",
    );
    write(
        dir.path(),
        &format!("{SLICES}/cancel_order/internal/cancel_order_handler.rs"),
        "use crate::domain::aggregate_order::order_aggregate::decide;\n\
         use crate::ports::event_store_port::EventStorePort;\n",
    );
    write(
        dir.path(),
        &format!("{SLICES}/cancel_order/internal/cancel_order_handler_test.rs"),
        "#[test]\nfn t() {}\n",
    );
    validate(dir.path()).success();
}

#[test]
fn view_slice_without_projection_fails() {
    let dir = workspace();
    write(
        dir.path(),
        &format!("{SLICES}/list_orders/slice.yaml"),
        "name: list_orders\ntype: query\n",
    );
    write(
        dir.path(),
        &format!("{SLICES}/list_orders/list_orders_handler.rs"),
        "pub fn list() {}\n",
    );
    write(
        dir.path(),
        &format!("{SLICES}/list_orders/list_orders_handler_test.rs"),
        "#[test]\nfn t() {}\n",
    );
    validate(dir.path()).failure().stdout(contains("slice 'list_orders' is missing a projection"));
}
