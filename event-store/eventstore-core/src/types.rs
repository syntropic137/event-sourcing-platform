use std::pin::Pin;

use crate::errors::StoreError;
use tokio_stream::Stream;

pub use eventstore_proto::gen as proto;

pub type StoreStream<T> = Pin<Box<dyn Stream<Item = Result<T, StoreError>> + Send>>;
