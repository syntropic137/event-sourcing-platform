//! Enhanced validation system for VSA

mod bounded_context_rules;
mod integration_event_rules;
mod rules;
mod suggestions;

pub use bounded_context_rules::{
    ContextBoundariesRule, NoCircularDependenciesRule, RequireSharedFolderRule,
};
pub use integration_event_rules::{
    IntegrationEventNamingRule, IntegrationEventsLocationRule, NoDuplicateIntegrationEventsRule,
};
pub use rules::{ValidationRule, ValidationRuleSet};
pub use suggestions::{Suggestion, SuggestionAction};

use crate::config::VsaConfig;
use std::path::PathBuf;

/// Enhanced validation report with suggestions
#[derive(Debug, Default)]
pub struct EnhancedValidationReport {
    pub errors: Vec<ValidationIssue>,
    pub warnings: Vec<ValidationIssue>,
    pub suggestions: Vec<Suggestion>,
}

/// Validation issue with detailed information
#[derive(Debug, Clone)]
pub struct ValidationIssue {
    pub path: PathBuf,
    pub message: String,
    pub code: String,
    pub severity: Severity,
    pub suggestions: Vec<Suggestion>,
}

/// Issue severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl EnhancedValidationReport {
    /// Check if validation passed (no errors)
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    /// Get total issue count
    pub fn total_issues(&self) -> usize {
        self.errors.len() + self.warnings.len()
    }

    /// Check if there are any suggestions
    pub fn has_suggestions(&self) -> bool {
        !self.suggestions.is_empty()
            || self.errors.iter().any(|e| !e.suggestions.is_empty())
            || self.warnings.iter().any(|w| !w.suggestions.is_empty())
    }
}

/// Validation context for passing state between rules
#[derive(Debug)]
pub struct ValidationContext {
    pub config: VsaConfig,
    pub root: PathBuf,
}

impl ValidationContext {
    pub fn new(config: VsaConfig, root: PathBuf) -> Self {
        Self { config, root }
    }
}
