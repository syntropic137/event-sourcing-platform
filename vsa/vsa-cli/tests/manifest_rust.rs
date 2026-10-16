//! `vsa manifest --include-domain` on a hexagonal Rust project: commands and
//! events are enum variants, each command links to the events its decision
//! builds, and a context's features are its slices only (#459).

mod common;

use assert_cmd::Command;
use common::{workspace, write};
use std::path::Path;

fn manifest(dir: &Path) -> serde_json::Value {
    let out = dir.join("manifest.json");
    Command::cargo_bin("vsa")
        .unwrap()
        .current_dir(dir)
        .args(["manifest", "-c", "vsa.yaml", "--include-domain", "--output"])
        .arg(&out)
        .assert()
        .success();
    serde_json::from_str(&std::fs::read_to_string(out).unwrap()).unwrap()
}

fn names(values: &serde_json::Value) -> Vec<String> {
    values.as_array().unwrap().iter().map(|v| v["name"].as_str().unwrap().to_string()).collect()
}

#[test]
fn commands_and_events_are_variants_linked_by_the_decision() {
    let dir = workspace();
    write(
        dir.path(),
        "contexts/orders/domain/commands/order_command.rs",
        "pub enum OrderCommand {\n    Place { id: u64, note: Option<String> },\n    Cancel,\n}\n",
    );
    write(
        dir.path(),
        "contexts/orders/domain/events/order_event.rs",
        "pub enum OrderEvent {\n    Placed { id: u64 },\n    Cancelled,\n}\n",
    );
    write(
        dir.path(),
        "contexts/orders/domain/aggregate_order/order_aggregate.rs",
        "use crate::domain::commands::order_command::OrderCommand;\n\
         use crate::domain::events::order_event::OrderEvent;\n\
         pub fn decide(command: OrderCommand) -> Vec<OrderEvent> {\n\
             match command {\n\
                 OrderCommand::Place { id, .. } => vec![OrderEvent::Placed { id }],\n\
                 OrderCommand::Cancel => cancel(),\n\
             }\n\
         }\n\
         fn cancel() -> Vec<OrderEvent> { vec![OrderEvent::Cancelled] }\n",
    );
    let m = manifest(dir.path());
    let domain = &m["domain"];
    assert_eq!(names(&domain["commands"]), ["OrderCommand::Place", "OrderCommand::Cancel"]);
    assert_eq!(names(&domain["events"]), ["OrderEvent::Placed", "OrderEvent::Cancelled"]);
    let place = &domain["commands"][0]["fields"];
    assert_eq!(place[0]["name"], "id");
    assert_eq!(place[1]["required"], false, "Option<_> is optional");
    let links = &domain["relationships"]["command_to_events"];
    assert_eq!(links["OrderCommand::Place"], serde_json::json!(["OrderEvent::Placed"]));
    assert_eq!(links["OrderCommand::Cancel"], serde_json::json!(["OrderEvent::Cancelled"]));
    assert_eq!(
        domain["relationships"]["command_to_aggregate"]["OrderCommand::Cancel"],
        "order_aggregate"
    );
}

#[test]
fn features_are_the_slices_with_their_types() {
    let dir = workspace();
    // Layers and a slice's internal folder are not features.
    write(dir.path(), "contexts/orders/slices/place_order/internal/helper.rs", "pub fn h() {}\n");
    write(dir.path(), "contexts/orders/application/mod.rs", "\n");
    let m = manifest(dir.path());
    let features = &m["bounded_contexts"][0]["features"];
    assert_eq!(names(features), ["place_order"], "{features}");
    assert_eq!(features[0]["slice_type"], "command");
    assert_eq!(features[0]["slice_types"], serde_json::json!(["command"]));
    assert_eq!(features[0]["path"], "slices/place_order");
}

#[test]
fn a_command_decided_in_several_arms_gets_every_event() {
    let dir = workspace();
    write(
        dir.path(),
        "contexts/orders/domain/events/order_event.rs",
        "pub enum OrderEvent {\n    Placed { id: u64 },\n    Flagged,\n}\n",
    );
    write(
        dir.path(),
        "contexts/orders/domain/aggregate_order/order_aggregate.rs",
        "pub fn decide(placed: bool, command: OrderCommand) -> Vec<OrderEvent> {\n\
             match command {\n\
                 OrderCommand::Place { id } if !placed => vec![OrderEvent::Placed { id }],\n\
                 OrderCommand::Place { .. } => vec![OrderEvent::Flagged],\n\
             }\n\
         }\n",
    );
    let m = manifest(dir.path());
    assert_eq!(
        m["domain"]["relationships"]["command_to_events"]["OrderCommand::Place"],
        serde_json::json!(["OrderEvent::Flagged", "OrderEvent::Placed"])
    );
}
