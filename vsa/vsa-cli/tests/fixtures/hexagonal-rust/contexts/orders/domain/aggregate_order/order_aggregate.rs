//! The order aggregate: decides commands against state.
use crate::domain::commands::order_command::OrderCommand;
use crate::domain::events::order_event::OrderEvent;

pub fn decide(placed: bool, command: OrderCommand) -> Result<Vec<OrderEvent>, String> {
    match command {
        OrderCommand::Place { id } if !placed => Ok(vec![OrderEvent::Placed { id }]),
        OrderCommand::Place { .. } => Err("already placed".to_string()),
    }
}
