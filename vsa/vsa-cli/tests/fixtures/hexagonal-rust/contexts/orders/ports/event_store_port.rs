use crate::domain::events::order_event::OrderEvent;

pub trait EventStorePort {
    fn append(&mut self, events: &[OrderEvent]);
}
