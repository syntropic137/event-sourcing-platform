//! Slice types as Event Modeling patterns, through the real `vsa validate`.
//! The fixture allows `slices.types: [command]`. On v0.18.0 the first two
//! probes exited 0.

mod common;

use common::{set_validation_line, validate, workspace, write};
use predicates::str::contains;

const PLACE: &str = "contexts/orders/slices/place_order/slice.yaml";

#[test]
fn unknown_slice_type_fails() {
    let dir = workspace();
    write(dir.path(), PLACE, "name: place_order\ntype: bogus\n");
    validate(dir.path()).failure().stdout(contains("declares unknown type 'bogus'"));
}

#[test]
fn slice_type_outside_slices_types_fails() {
    let dir = workspace();
    write(dir.path(), PLACE, "name: place_order\ntype: saga\n");
    validate(dir.path())
        .failure()
        .stdout(contains("is type 'saga', which slices.types [command] does not allow"));
}

#[test]
fn deprecated_names_warn_and_count_against_the_budget() {
    let dir = workspace();
    set_validation_line(dir.path(), "  types: [command]", "  types: [command, query]");
    validate(dir.path())
        .failure()
        .stdout(contains("slices.types lists 'query', the deprecated name for 'view'"));
    set_validation_line(dir.path(), "  types: [command, query]", "  types: [command, view]");
    validate(dir.path()).success();
}

#[test]
fn automation_slice_is_a_todo_view_plus_a_command() {
    let dir = workspace();
    set_validation_line(dir.path(), "  types: [command]", "  types: [command, view, automation]");
    let slice = "contexts/orders/slices/ship_paid_orders";
    write(dir.path(), &format!("{slice}/slice.yaml"), "name: ship_paid_orders\ntype: automation\n");
    // Event in, event out: rejected.
    write(dir.path(), &format!("{slice}/relay.rs"), "pub fn on_paid() {}\n");
    validate(dir.path())
        .failure()
        .stdout(contains("automation slice 'ship_paid_orders' is missing"));
    // A to-do list of paid-but-unshipped orders, and a processor that issues the command.
    write(
        dir.path(),
        &format!("{slice}/paid_unshipped_projection.rs"),
        "pub struct PaidUnshipped;\n",
    );
    write(
        dir.path(),
        &format!("{slice}/relay.rs"),
        "use crate::domain::commands::order_command::OrderCommand;\npub fn next() -> OrderCommand { todo!() }\n",
    );
    write(dir.path(), &format!("{slice}/relay_test.rs"), "#[test]\nfn t() {}\n");
    validate(dir.path()).success();
}

#[test]
fn projection_runner_that_issues_commands_is_not_a_view() {
    let dir = workspace();
    set_validation_line(dir.path(), "  types: [command]", "  types: [command, view]");
    let slice = "contexts/orders/slices/order_totals";
    write(dir.path(), &format!("{slice}/slice.yaml"), "name: order_totals\ntype: view\n");
    write(dir.path(), &format!("{slice}/order_totals_handler.rs"), "pub fn get() {}\n");
    write(dir.path(), &format!("{slice}/order_totals_projection.rs"), "pub struct Totals;\n");
    write(dir.path(), &format!("{slice}/order_totals_projection_test.rs"), "#[test]\nfn t() {}\n");
    validate(dir.path()).success();
    write(
        dir.path(),
        &format!("{slice}/order_totals_projection.rs"),
        "use crate::domain::commands::order_command::OrderCommand;\npub struct Totals;\n",
    );
    validate(dir.path()).failure().stdout(contains("View slice 'order_totals' issues a command"));
}
