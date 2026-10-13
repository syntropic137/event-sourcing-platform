use crate::domain::commands::order_command::OrderCommand;
use crate::domain::aggregate_order::order_aggregate::decide;
use crate::ports::event_store_port::EventStorePort;

pub fn place_order(id: u64, store: &mut dyn EventStorePort) -> Result<(), String> {
    let events = decide(false, OrderCommand::Place { id })?;
    store.append(&events);
    Ok(())
}
