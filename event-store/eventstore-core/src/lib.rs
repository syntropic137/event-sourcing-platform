pub mod errors;
pub mod trait_event_store;
pub mod types;

pub use errors::StoreError;
pub use eventstore_proto::{capabilities, API_VERSION, SERVER_INFO_MIN_VERSION};
pub use trait_event_store::EventStore;
pub use types::{proto, StoreStream};
