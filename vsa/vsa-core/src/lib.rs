//! # VSA Core
//!
//! Core library for Vertical Slice Architecture validation and management.
//!
//! This library provides:
//! - Configuration parsing and validation
//! - File system scanning and pattern matching
//! - Bounded context boundary validation
//! - Integration event duplication detection
//! - Framework integration support
//! - Manifest generation

pub mod bounded_contexts;
pub mod config;
pub mod domain;
pub mod error;
pub mod framework;
pub mod integration_events;
pub mod manifest;
pub mod patterns;
pub mod scanner;
pub mod scanners;
pub mod validation;
pub mod validator;

pub use config::{
    AggregateConfig, ArchitectureType, ArchitectureValidation, CommandConfig, CommandSliceConfig,
    ContextConfig, CqrsValidation, DecoratorValidation, DomainConfig, DomainValidation,
    EventConfig, EventSourcingValidation, EventVersioningConfig, InfrastructureConfig,
    LanguageConfig, QueryConfig, QuerySliceConfig, SagaSliceConfig, SliceType, SliceValidation,
    SlicesConfig, ValidationConfig, VersionFormat, VsaConfig,
};
pub use domain::{
    Aggregate, Command, CommandField, CommandHandler, DomainModel, Event, EventField, EventHandler,
    EventVersion, Query, QueryField, Upcaster,
};
pub use error::{Result, VsaError};
pub use integration_events::{IntegrationEvent, IntegrationEventRegistry};
pub use manifest::Manifest;
pub use scanner::Scanner;
pub use scanners::{AggregateScanner, CommandScanner, DomainScanner, EventScanner, QueryScanner};
pub use validation::{
    EnhancedValidationReport, Severity, Suggestion, SuggestionAction, ValidationContext,
    ValidationIssue, ValidationRule, ValidationRuleSet,
};
pub use validator::Validator;

/// VSA library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert!(!VERSION.is_empty());
    }
}
