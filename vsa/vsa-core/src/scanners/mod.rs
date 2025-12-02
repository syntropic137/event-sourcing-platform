//! Domain and slice scanners
//!
//! This module contains scanners for extracting metadata from the codebase:
//! - Domain scanner: Scans the domain/ folder
//! - Aggregate scanner: Finds and analyzes aggregates
//! - Command scanner: Finds commands
//! - Query scanner: Finds queries
//! - Event scanner: Finds events and their versions
//! - Upcaster scanner: Finds upcasters
//! - Projection scanner: Finds projections (CQRS read models)
//! - Slice scanner: Finds and analyzes vertical slices

pub mod aggregate_scanner;
pub mod command_scanner;
pub mod domain_scanner;
pub mod event_scanner;
pub mod projection_scanner;
pub mod query_scanner;
pub mod slice_scanner;

pub use aggregate_scanner::AggregateScanner;
pub use command_scanner::CommandScanner;
pub use domain_scanner::DomainScanner;
pub use event_scanner::EventScanner;
pub use projection_scanner::ProjectionScanner;
pub use query_scanner::QueryScanner;
pub use slice_scanner::{Slice, SliceFile, SliceFileType, SliceManifest, SliceScanner};
