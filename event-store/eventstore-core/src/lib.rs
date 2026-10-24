#[cfg(feature = "conformance")]
pub mod conformance;
pub mod errors;
pub mod fingerprint;
pub mod paging;
pub mod trait_event_store;
pub mod types;

pub use errors::StoreError;
pub use eventstore_proto::{capabilities, API_VERSION, SERVER_INFO_MIN_VERSION};
pub use trait_event_store::EventStore;
pub use types::{proto, StoreStream};
