//! Domain and slice scanners
//!
//! This module contains scanners for extracting metadata from the codebase:
//! - Domain scanner: Scans the domain/ folder
//! - Aggregate scanner: Finds and analyzes aggregates
//! - Command scanner: Finds commands
//! - Query scanner: Finds queries
//! - Event scanner: Finds events and their versions
//! - Upcaster scanner: Finds upcasters

pub mod aggregate_scanner;
pub mod command_scanner;
pub mod domain_scanner;
pub mod event_scanner;
pub mod query_scanner;

pub use aggregate_scanner::AggregateScanner;
pub use command_scanner::CommandScanner;
pub use domain_scanner::DomainScanner;
pub use event_scanner::EventScanner;
pub use query_scanner::QueryScanner;
