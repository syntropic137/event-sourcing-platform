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

use crate::config::FilenameConvention;

/// Does `stem` (a filename with the extension already stripped) carry the
/// artifact `suffix` under the given filename convention?
///
/// - `pascal_case` (default): the stem must end with `suffix` (e.g.
///   `CreateFooCommand`) - the TypeScript/Python idiom, unchanged behavior.
/// - `snake_case`: the stem must end with `_<suffix lowercased>` (e.g.
///   `create_foo_command`) - the idiomatic Rust convention.
///
/// This mirrors the detection logic in `validation::structure_rules` so
/// `vsa manifest --include-domain` and `vsa validate` agree on what counts as
/// a domain artifact.
pub(crate) fn stem_matches_suffix(
    stem: &str,
    suffix: &str,
    convention: &FilenameConvention,
) -> bool {
    match convention {
        FilenameConvention::PascalCase => stem.ends_with(suffix),
        FilenameConvention::SnakeCase => stem.ends_with(&format!("_{}", suffix.to_lowercase())),
    }
}

pub use aggregate_scanner::AggregateScanner;
pub use command_scanner::CommandScanner;
pub use domain_scanner::DomainScanner;
pub use event_scanner::EventScanner;
pub use projection_scanner::ProjectionScanner;
pub use query_scanner::QueryScanner;
pub use slice_scanner::{Slice, SliceFile, SliceFileType, SliceManifest, SliceScanner};
