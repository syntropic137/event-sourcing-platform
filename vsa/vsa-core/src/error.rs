//! Error types for VSA Core

use std::path::PathBuf;
use thiserror::Error;

/// Result type alias for VSA operations
pub type Result<T> = std::result::Result<T, VsaError>;

/// VSA error types
#[derive(Error, Debug)]
pub enum VsaError {
    /// Configuration file not found
    #[error("Configuration file not found: {0}")]
    ConfigNotFound(PathBuf),

    /// Invalid configuration
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    /// YAML parsing error
    #[error("YAML parsing error: {0}")]
    YamlError(#[from] serde_yaml::Error),

    /// JSON error
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    /// IO error
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    /// Pattern matching error
    #[error("Pattern error: {0}")]
    PatternError(#[from] regex::Error),

    /// Validation error
    #[error("Validation error: {0}")]
    ValidationError(String),

    /// Invalid file structure
    #[error("Invalid structure at {path}: {reason}")]
    InvalidStructure { path: PathBuf, reason: String },

    /// Duplicate integration event
    #[error("Duplicate integration event '{event}' found in {contexts:?}")]
    DuplicateIntegrationEvent { event: String, contexts: Vec<String> },

    /// Missing required file
    #[error("Missing required file: {0}")]
    MissingFile(PathBuf),

    /// Invalid pattern
    #[error("Invalid pattern '{pattern}' in file: {file}")]
    InvalidPattern { pattern: String, file: PathBuf },

    /// Framework integration error
    #[error("Framework integration error: {0}")]
    FrameworkError(String),

    /// Template error
    #[error("Template error: {0}")]
    TemplateError(String),

    /// Context not found
    #[error("Context '{0}' not found")]
    ContextNotFound(String),

    /// Feature not found
    #[error("Feature '{feature}' not found in context '{context}'")]
    FeatureNotFound { context: String, feature: String },

    /// Invalid operation name
    #[error("Invalid operation name: {0}")]
    InvalidOperationName(String),

    /// Unsupported language
    #[error("Unsupported language: {0}")]
    UnsupportedLanguage(String),
}
