//! Configuration parsing and validation

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::{Result, VsaError};

/// VSA configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VsaConfig {
    /// Configuration version (now supports v2)
    pub version: u32,

    /// Architecture type
    #[serde(default)]
    pub architecture: ArchitectureType,

    /// Root directory for contexts (relative to config file)
    pub root: PathBuf,

    /// Primary language
    pub language: String,

    /// Domain layer configuration (NEW in v2)
    #[serde(default)]
    pub domain: Option<DomainConfig>,

    /// Slices layer configuration (NEW in v2)
    #[serde(default)]
    pub slices: Option<SlicesConfig>,

    /// Infrastructure layer configuration (NEW in v2)
    #[serde(default)]
    pub infrastructure: Option<InfrastructureConfig>,

    /// Optional framework integration
    #[serde(default)]
    pub framework: Option<FrameworkConfig>,

    /// Context-specific configuration (legacy/advanced)
    #[serde(default)]
    pub contexts: HashMap<String, ContextConfig>,

    /// Validation rules (ENHANCED in v2)
    #[serde(default)]
    pub validation: ValidationConfig,

    /// Pattern definitions
    #[serde(default)]
    pub patterns: PatternsConfig,
}

/// Architecture type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum ArchitectureType {
    /// Basic vertical slice architecture (legacy)
    VerticalSlice,
    /// Hexagonal architecture
    Hexagonal,
    /// Hexagonal Event-Sourced VSA (recommended)
    HexagonalEventSourcedVsa,
}

impl Default for ArchitectureType {
    fn default() -> Self {
        Self::VerticalSlice
    }
}

// ============================================================================
// DOMAIN LAYER CONFIGURATION
// ============================================================================

/// Domain layer configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainConfig {
    /// Path to domain folder (relative to root)
    #[serde(default = "default_domain_path")]
    pub path: PathBuf,

    /// Aggregates configuration
    #[serde(default)]
    pub aggregates: AggregateConfig,

    /// Commands configuration
    #[serde(default)]
    pub commands: CommandConfig,

    /// Queries configuration
    #[serde(default)]
    pub queries: QueryConfig,

    /// Events configuration
    #[serde(default)]
    pub events: EventConfig,
}

impl Default for DomainConfig {
    fn default() -> Self {
        Self {
            path: default_domain_path(),
            aggregates: AggregateConfig::default(),
            commands: CommandConfig::default(),
            queries: QueryConfig::default(),
            events: EventConfig::default(),
        }
    }
}

/// Aggregate configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregateConfig {
    /// Path within domain/ where aggregates are stored
    #[serde(default = "default_dot_path")]
    pub path: PathBuf,

    /// File naming pattern (glob)
    #[serde(default = "default_aggregate_pattern")]
    pub pattern: String,

    /// Require "Aggregate" suffix in filename
    #[serde(default = "default_true")]
    pub require_suffix: bool,

    /// File extensions to scan
    #[serde(default = "default_extensions")]
    pub extensions: Vec<String>,
}

impl Default for AggregateConfig {
    fn default() -> Self {
        Self {
            path: default_dot_path(),
            pattern: default_aggregate_pattern(),
            require_suffix: true,
            extensions: default_extensions(),
        }
    }
}

/// Command configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandConfig {
    /// Path within domain/ where commands are stored
    #[serde(default = "default_commands_path")]
    pub path: PathBuf,

    /// File naming pattern (glob)
    #[serde(default = "default_command_pattern")]
    pub pattern: String,

    /// Require "Command" suffix
    #[serde(default = "default_true")]
    pub require_suffix: bool,

    /// Require aggregateId field in all commands
    #[serde(default = "default_true")]
    pub require_aggregate_id: bool,

    /// Organize commands by feature (commands/{feature}/)
    #[serde(default = "default_true")]
    pub organize_by_feature: bool,

    /// File extensions
    #[serde(default = "default_extensions")]
    pub extensions: Vec<String>,
}

impl Default for CommandConfig {
    fn default() -> Self {
        Self {
            path: default_commands_path(),
            pattern: default_command_pattern(),
            require_suffix: true,
            require_aggregate_id: true,
            organize_by_feature: true,
            extensions: default_extensions(),
        }
    }
}

/// Query configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryConfig {
    /// Path within domain/ where queries are stored
    #[serde(default = "default_queries_path")]
    pub path: PathBuf,

    /// File naming pattern (glob)
    #[serde(default = "default_query_pattern")]
    pub pattern: String,

    /// Require "Query" suffix
    #[serde(default = "default_true")]
    pub require_suffix: bool,

    /// Organize queries by feature
    #[serde(default = "default_true")]
    pub organize_by_feature: bool,

    /// File extensions
    #[serde(default = "default_extensions")]
    pub extensions: Vec<String>,
}

impl Default for QueryConfig {
    fn default() -> Self {
        Self {
            path: default_queries_path(),
            pattern: default_query_pattern(),
            require_suffix: true,
            organize_by_feature: true,
            extensions: default_extensions(),
        }
    }
}

/// Event configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventConfig {
    /// Path within domain/ where events are stored
    #[serde(default = "default_events_path")]
    pub path: PathBuf,

    /// File naming pattern (glob)
    #[serde(default = "default_event_pattern")]
    pub pattern: String,

    /// Require "Event" suffix
    #[serde(default = "default_true")]
    pub require_suffix: bool,

    /// File extensions
    #[serde(default = "default_extensions")]
    pub extensions: Vec<String>,

    /// Event versioning configuration
    #[serde(default)]
    pub versioning: EventVersioningConfig,
}

impl Default for EventConfig {
    fn default() -> Self {
        Self {
            path: default_events_path(),
            pattern: default_event_pattern(),
            require_suffix: true,
            extensions: default_extensions(),
            versioning: EventVersioningConfig::default(),
        }
    }
}

/// Event versioning configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventVersioningConfig {
    /// Enable event versioning validation
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Version format
    #[serde(default)]
    pub format: VersionFormat,

    /// Require @Event decorator with version parameter
    #[serde(default = "default_true")]
    pub require_decorator: bool,

    /// Require upcasters when event schema changes
    #[serde(default = "default_true")]
    pub require_upcasters: bool,

    /// Path for old event versions (relative to events/)
    #[serde(default = "default_versioned_path")]
    pub versioned_path: PathBuf,

    /// Path for upcaster implementations (relative to events/)
    #[serde(default = "default_upcasters_path")]
    pub upcasters_path: PathBuf,

    /// Upcaster naming pattern
    #[serde(default = "default_upcaster_pattern")]
    pub upcaster_pattern: String,
}

impl Default for EventVersioningConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            format: VersionFormat::default(),
            require_decorator: true,
            require_upcasters: true,
            versioned_path: default_versioned_path(),
            upcasters_path: default_upcasters_path(),
            upcaster_pattern: default_upcaster_pattern(),
        }
    }
}

/// Version format
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum VersionFormat {
    /// Simple string-based versions ('v1', 'v2', 'v3')
    Simple,
    /// Semantic versioning ('1.0.0', '1.1.0', '2.0.0')
    Semver,
}

impl Default for VersionFormat {
    fn default() -> Self {
        Self::Simple
    }
}

// ============================================================================
// SLICES LAYER CONFIGURATION
// ============================================================================

/// Slices layer configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlicesConfig {
    /// Path to slices folder (relative to root)
    #[serde(default = "default_slices_path")]
    pub path: PathBuf,

    /// Slice types to validate
    #[serde(default = "default_slice_types")]
    pub types: Vec<SliceType>,

    /// Slice metadata file name
    #[serde(default = "default_slice_metadata_file")]
    pub metadata_file: String,

    /// Command slice configuration
    #[serde(default)]
    pub command: Option<CommandSliceConfig>,

    /// Query slice configuration
    #[serde(default)]
    pub query: Option<QuerySliceConfig>,

    /// Saga slice configuration
    #[serde(default)]
    pub saga: Option<SagaSliceConfig>,
}

impl Default for SlicesConfig {
    fn default() -> Self {
        Self {
            path: default_slices_path(),
            types: default_slice_types(),
            metadata_file: default_slice_metadata_file(),
            command: Some(CommandSliceConfig::default()),
            query: Some(QuerySliceConfig::default()),
            saga: Some(SagaSliceConfig::default()),
        }
    }
}

/// Slice type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SliceType {
    /// Command slice (write operations)
    Command,
    /// Query slice (read operations)
    Query,
    /// Saga slice (process managers)
    Saga,
}

/// Command slice configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandSliceConfig {
    /// Naming pattern for command slices
    #[serde(default = "default_wildcard_pattern")]
    pub pattern: String,

    /// Must dispatch via CommandBus
    #[serde(default = "default_command_bus")]
    pub must_use: String,

    /// Maximum lines of code (thin adapter enforcement)
    #[serde(default = "default_max_slice_lines")]
    pub max_lines: Option<usize>,

    /// Require tests for command slices
    #[serde(default = "default_true")]
    pub require_tests: bool,

    /// Prohibit business logic in command slices
    #[serde(default = "default_true")]
    pub no_business_logic: bool,

    /// Allowed adapter types
    #[serde(default = "default_adapter_types")]
    pub adapters: Vec<String>,
}

impl Default for CommandSliceConfig {
    fn default() -> Self {
        Self {
            pattern: default_wildcard_pattern(),
            must_use: default_command_bus(),
            max_lines: default_max_slice_lines(),
            require_tests: true,
            no_business_logic: true,
            adapters: default_adapter_types(),
        }
    }
}

/// Query slice configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuerySliceConfig {
    /// Naming pattern for query slices
    #[serde(default = "default_wildcard_pattern")]
    pub pattern: String,

    /// Require projection/read model
    #[serde(default = "default_true")]
    pub require_projection: bool,

    /// Must use QueryBus
    #[serde(default = "default_query_bus")]
    pub must_use: String,

    /// Maximum lines of code
    #[serde(default = "default_max_query_slice_lines")]
    pub max_lines: Option<usize>,

    /// Require tests
    #[serde(default = "default_true")]
    pub require_tests: bool,

    /// Allowed adapter types
    #[serde(default = "default_adapter_types")]
    pub adapters: Vec<String>,
}

impl Default for QuerySliceConfig {
    fn default() -> Self {
        Self {
            pattern: default_wildcard_pattern(),
            require_projection: true,
            must_use: default_query_bus(),
            max_lines: default_max_query_slice_lines(),
            require_tests: true,
            adapters: default_adapter_types(),
        }
    }
}

/// Saga slice configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SagaSliceConfig {
    /// Naming pattern for saga slices
    #[serde(default = "default_saga_pattern")]
    pub pattern: String,

    /// Must subscribe via EventBus
    #[serde(default = "default_event_bus")]
    pub must_use: String,

    /// Allow saga to send commands
    #[serde(default = "default_true")]
    pub can_send_commands: bool,

    /// Require error handling
    #[serde(default = "default_true")]
    pub require_error_handling: bool,

    /// Require tests
    #[serde(default = "default_true")]
    pub require_tests: bool,
}

impl Default for SagaSliceConfig {
    fn default() -> Self {
        Self {
            pattern: default_saga_pattern(),
            must_use: default_event_bus(),
            can_send_commands: true,
            require_error_handling: true,
            require_tests: true,
        }
    }
}

// ============================================================================
// INFRASTRUCTURE LAYER CONFIGURATION
// ============================================================================

/// Infrastructure layer configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfrastructureConfig {
    /// Path to infrastructure folder (relative to root)
    #[serde(default = "default_infrastructure_path")]
    pub path: PathBuf,

    /// Allowed infrastructure components
    #[serde(default = "default_allowed_infrastructure")]
    pub allowed: Vec<String>,
}

impl Default for InfrastructureConfig {
    fn default() -> Self {
        Self { path: default_infrastructure_path(), allowed: default_allowed_infrastructure() }
    }
}

// ============================================================================
// FRAMEWORK INTEGRATION CONFIGURATION
// ============================================================================

/// Framework integration configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameworkConfig {
    /// Framework name
    pub name: String,

    /// Base types configuration
    #[serde(default)]
    pub base_types: HashMap<String, BaseTypeConfig>,
}

/// Base type configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseTypeConfig {
    /// Import path
    pub import: String,

    /// Class/type name
    pub class: String,
}

/// Context-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContextConfig {
    /// Context description
    pub description: Option<String>,

    /// Optional features to disable validation for
    #[serde(default)]
    pub optional_features: Vec<String>,

    /// Custom patterns for this context
    #[serde(default)]
    pub patterns: Option<PatternsConfig>,
}

/// Validation configuration (ENHANCED for v2)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationConfig {
    // -------------------------------------------------------------------------
    // Hexagonal Architecture Validation (NEW in v2)
    // -------------------------------------------------------------------------
    /// Architecture validation rules
    #[serde(default)]
    pub architecture: Option<ArchitectureValidation>,

    // -------------------------------------------------------------------------
    // CQRS Validation (NEW in v2)
    // -------------------------------------------------------------------------
    /// CQRS validation rules
    #[serde(default)]
    pub cqrs: Option<CqrsValidation>,

    // -------------------------------------------------------------------------
    // Event Sourcing Validation (NEW in v2)
    // -------------------------------------------------------------------------
    /// Event sourcing validation rules
    #[serde(default)]
    pub event_sourcing: Option<EventSourcingValidation>,

    // -------------------------------------------------------------------------
    // Decorator Validation (NEW in v2)
    // -------------------------------------------------------------------------
    /// Decorator validation rules
    #[serde(default)]
    pub decorators: Option<DecoratorValidation>,

    // -------------------------------------------------------------------------
    // Domain Organization Validation (NEW in v2)
    // -------------------------------------------------------------------------
    /// Domain organization validation rules
    #[serde(default)]
    pub domain: Option<DomainValidation>,

    // -------------------------------------------------------------------------
    // Slice Validation (NEW in v2)
    // -------------------------------------------------------------------------
    /// Slice validation rules
    #[serde(default)]
    pub slices: Option<SliceValidation>,

    // -------------------------------------------------------------------------
    // General Validation (legacy/existing)
    // -------------------------------------------------------------------------
    /// Require tests for all operations
    #[serde(default = "default_true")]
    pub require_tests: bool,

    /// Require handler for each command/query/event
    #[serde(default = "default_true")]
    pub require_handler: bool,

    /// Allow unknown files in project
    #[serde(default = "default_true")]
    pub allow_unknown_files: bool,

    /// Enforce bounded context boundaries
    #[serde(default = "default_true")]
    pub enforce_boundaries: bool,

    /// Require integration events in _shared (legacy)
    #[serde(default = "default_true")]
    pub require_integration_events_in_shared: bool,

    /// Maximum nesting depth for features (legacy)
    #[serde(default = "default_max_depth")]
    pub max_nesting_depth: usize,

    /// Allow nested features (legacy)
    #[serde(default = "default_true")]
    pub allow_nested_features: bool,

    /// Maximum warnings before failing validation
    #[serde(default = "default_max_warnings")]
    pub max_warnings: Option<usize>,

    /// Fail on errors
    #[serde(default = "default_true")]
    pub fail_on_errors: bool,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            architecture: None,
            cqrs: None,
            event_sourcing: None,
            decorators: None,
            domain: None,
            slices: None,
            require_tests: true,
            require_handler: true,
            allow_unknown_files: true,
            enforce_boundaries: true,
            require_integration_events_in_shared: true,
            max_nesting_depth: 3,
            allow_nested_features: true,
            max_warnings: Some(10),
            fail_on_errors: true,
        }
    }
}

/// Architecture validation rules
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchitectureValidation {
    /// Enforce hexagonal architecture rules
    #[serde(default = "default_true")]
    pub enforce_hexagonal: bool,

    /// Domain has no outward dependencies
    #[serde(default = "default_true")]
    pub domain_isolated: bool,

    /// Slices cannot contain business logic
    #[serde(default = "default_true")]
    pub no_business_logic_in_slices: bool,

    /// Slices are isolated from each other
    #[serde(default = "default_true")]
    pub slices_isolated: bool,

    /// Slices can only READ from domain
    #[serde(default = "default_true")]
    pub domain_read_only_from_slices: bool,

    /// Dependencies point inward (Adapter → Infrastructure → Domain)
    #[serde(default = "default_true")]
    pub enforce_dependency_direction: bool,
}

impl Default for ArchitectureValidation {
    fn default() -> Self {
        Self {
            enforce_hexagonal: true,
            domain_isolated: true,
            no_business_logic_in_slices: true,
            slices_isolated: true,
            domain_read_only_from_slices: true,
            enforce_dependency_direction: true,
        }
    }
}

/// CQRS validation rules
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CqrsValidation {
    /// Enforce CQRS separation
    #[serde(default = "default_true")]
    pub enforce_separation: bool,

    /// Commands use CommandBus
    #[serde(default = "default_true")]
    pub commands_use_command_bus: bool,

    /// Queries use QueryBus
    #[serde(default = "default_true")]
    pub queries_use_query_bus: bool,

    /// Commands return void
    #[serde(default = "default_false")]
    pub commands_return_void: bool,

    /// Queries don't modify state
    #[serde(default = "default_false")]
    pub queries_read_only: bool,
}

impl Default for CqrsValidation {
    fn default() -> Self {
        Self {
            enforce_separation: true,
            commands_use_command_bus: true,
            queries_use_query_bus: true,
            commands_return_void: false,
            queries_read_only: false,
        }
    }
}

/// Event sourcing validation rules
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventSourcingValidation {
    /// Require event versioning
    #[serde(default = "default_true")]
    pub require_event_versioning: bool,

    /// Require upcasters when event schema changes
    #[serde(default = "default_true")]
    pub require_upcasters_for_changes: bool,

    /// Warn about missing @Deprecated decorator
    #[serde(default = "default_true")]
    pub warn_missing_deprecated: bool,

    /// Validate upcaster version consistency
    #[serde(default = "default_true")]
    pub validate_upcaster_versions: bool,
}

impl Default for EventSourcingValidation {
    fn default() -> Self {
        Self {
            require_event_versioning: true,
            require_upcasters_for_changes: true,
            warn_missing_deprecated: true,
            validate_upcaster_versions: true,
        }
    }
}

/// Decorator validation rules
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecoratorValidation {
    /// Require @Event decorator on all events
    #[serde(default = "default_true")]
    pub require_event_decorator: bool,

    /// Require version parameter in @Event decorator
    #[serde(default = "default_true")]
    pub require_version_parameter: bool,

    /// Require @Upcaster decorator on upcasters
    #[serde(default = "default_true")]
    pub require_upcaster_decorator: bool,

    /// Require controller decorators
    #[serde(default = "default_false")]
    pub require_controller_decorators: bool,

    /// Require routing decorators
    #[serde(default = "default_false")]
    pub require_routing_decorators: bool,
}

impl Default for DecoratorValidation {
    fn default() -> Self {
        Self {
            require_event_decorator: true,
            require_version_parameter: true,
            require_upcaster_decorator: true,
            require_controller_decorators: false,
            require_routing_decorators: false,
        }
    }
}

/// Domain organization validation rules
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainValidation {
    /// Require aggregates in domain/ folder
    #[serde(default = "default_true")]
    pub aggregates_in_domain: bool,

    /// Require commands in domain/commands/
    #[serde(default = "default_true")]
    pub commands_in_domain_commands: bool,

    /// Require queries in domain/queries/
    #[serde(default = "default_true")]
    pub queries_in_domain_queries: bool,

    /// Require events in domain/events/
    #[serde(default = "default_true")]
    pub events_in_domain_events: bool,

    /// Enforce naming conventions
    #[serde(default = "default_true")]
    pub enforce_naming_conventions: bool,

    /// Require @CommandHandler in aggregates
    #[serde(default = "default_true")]
    pub require_command_handlers: bool,
}

impl Default for DomainValidation {
    fn default() -> Self {
        Self {
            aggregates_in_domain: true,
            commands_in_domain_commands: true,
            queries_in_domain_queries: true,
            events_in_domain_events: true,
            enforce_naming_conventions: true,
            require_command_handlers: true,
        }
    }
}

/// Slice validation rules
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SliceValidation {
    /// Enforce thin adapter pattern
    #[serde(default = "default_true")]
    pub enforce_thin_adapters: bool,

    /// Detect business logic in slices
    #[serde(default = "default_true")]
    pub detect_business_logic: bool,

    /// Require slice type consistency
    #[serde(default = "default_true")]
    pub require_type_consistency: bool,

    /// Require tests
    #[serde(default = "default_true")]
    pub require_tests: bool,

    /// Require slice.yaml metadata
    #[serde(default = "default_false")]
    pub require_metadata: bool,
}

impl Default for SliceValidation {
    fn default() -> Self {
        Self {
            enforce_thin_adapters: true,
            detect_business_logic: true,
            require_type_consistency: true,
            require_tests: true,
            require_metadata: false,
        }
    }
}

/// Pattern configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternsConfig {
    /// Command pattern (e.g., "*Command.ts")
    #[serde(default = "default_command_pattern")]
    pub command: String,

    /// Event pattern (e.g., "*Event.ts")
    #[serde(default = "default_event_pattern")]
    pub event: String,

    /// Handler pattern (e.g., "*Handler.ts")
    #[serde(default = "default_handler_pattern")]
    pub handler: String,

    /// Query pattern (e.g., "*Query.ts")
    #[serde(default = "default_query_pattern")]
    pub query: String,

    /// Integration event pattern (e.g., "*IntegrationEvent.ts")
    #[serde(default = "default_integration_event_pattern")]
    pub integration_event: String,

    /// Test pattern (e.g., "*.test.ts")
    #[serde(default = "default_test_pattern")]
    pub test: String,
}

impl Default for PatternsConfig {
    fn default() -> Self {
        Self {
            command: default_command_pattern(),
            event: default_event_pattern(),
            handler: default_handler_pattern(),
            query: default_query_pattern(),
            integration_event: default_integration_event_pattern(),
            test: default_test_pattern(),
        }
    }
}

/// Language-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageConfig {
    /// File extension
    pub extension: String,

    /// Pattern overrides
    #[serde(default)]
    pub patterns: Option<PatternsConfig>,
}

// ============================================================================
// DEFAULT VALUE FUNCTIONS
// ============================================================================

// Boolean defaults
fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}

// Path defaults
fn default_dot_path() -> PathBuf {
    PathBuf::from(".")
}

fn default_domain_path() -> PathBuf {
    PathBuf::from("domain")
}

fn default_commands_path() -> PathBuf {
    PathBuf::from("commands")
}

fn default_queries_path() -> PathBuf {
    PathBuf::from("queries")
}

fn default_events_path() -> PathBuf {
    PathBuf::from("events")
}

fn default_slices_path() -> PathBuf {
    PathBuf::from("slices")
}

fn default_infrastructure_path() -> PathBuf {
    PathBuf::from("infrastructure")
}

fn default_versioned_path() -> PathBuf {
    PathBuf::from("_versioned")
}

fn default_upcasters_path() -> PathBuf {
    PathBuf::from("_upcasters")
}

// Pattern defaults
fn default_aggregate_pattern() -> String {
    "*Aggregate.*".to_string()
}

fn default_command_pattern() -> String {
    "**/*Command.*".to_string()
}

fn default_query_pattern() -> String {
    "**/*Query.*".to_string()
}

fn default_event_pattern() -> String {
    "**/*Event.*".to_string()
}

fn default_handler_pattern() -> String {
    "*Handler".to_string()
}

fn default_integration_event_pattern() -> String {
    "*IntegrationEvent".to_string()
}

fn default_test_pattern() -> String {
    "*.test".to_string()
}

fn default_upcaster_pattern() -> String {
    "*_Upcaster_*.*".to_string()
}

fn default_wildcard_pattern() -> String {
    "*".to_string()
}

fn default_saga_pattern() -> String {
    "*-saga".to_string()
}

// Infrastructure defaults
fn default_command_bus() -> String {
    "CommandBus".to_string()
}

fn default_query_bus() -> String {
    "QueryBus".to_string()
}

fn default_event_bus() -> String {
    "EventBus".to_string()
}

fn default_allowed_infrastructure() -> Vec<String> {
    vec![
        "CommandBus".to_string(),
        "QueryBus".to_string(),
        "EventBus".to_string(),
        "*Repository".to_string(),
        "*Gateway".to_string(),
        "*Service".to_string(),
    ]
}

// Slice defaults
fn default_slice_types() -> Vec<SliceType> {
    vec![SliceType::Command, SliceType::Query, SliceType::Saga]
}

fn default_slice_metadata_file() -> String {
    "slice.yaml".to_string()
}

fn default_max_slice_lines() -> Option<usize> {
    Some(50)
}

fn default_max_query_slice_lines() -> Option<usize> {
    Some(100)
}

fn default_adapter_types() -> Vec<String> {
    vec!["rest".to_string(), "cli".to_string(), "grpc".to_string(), "graphql".to_string()]
}

// Extension defaults
fn default_extensions() -> Vec<String> {
    vec![".ts".to_string(), ".tsx".to_string()]
}

// Numeric defaults
fn default_max_depth() -> usize {
    3
}

fn default_max_warnings() -> Option<usize> {
    Some(10)
}

impl VsaConfig {
    /// Load configuration from a YAML file
    pub fn from_file(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Err(VsaError::ConfigNotFound(path.to_path_buf()));
        }

        let content = std::fs::read_to_string(path)?;
        let config: VsaConfig = serde_yaml::from_str(&content)?;

        config.validate()?;

        Ok(config)
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<()> {
        // Support versions 1 and 2
        if self.version != 1 && self.version != 2 {
            return Err(VsaError::InvalidConfig(format!(
                "Unsupported version: {}. Expected: 1 or 2",
                self.version
            )));
        }

        // Validate language
        if !["typescript", "python", "rust"].contains(&self.language.as_str()) {
            return Err(VsaError::UnsupportedLanguage(self.language.clone()));
        }

        // V2-specific validation
        if self.version == 2 {
            // If using hexagonal architecture, domain config should be present
            if matches!(
                self.architecture,
                ArchitectureType::Hexagonal | ArchitectureType::HexagonalEventSourcedVsa
            ) {
                if self.domain.is_none() {
                    return Err(VsaError::InvalidConfig(
                        "Hexagonal architecture requires domain configuration".to_string(),
                    ));
                }
                if self.slices.is_none() {
                    return Err(VsaError::InvalidConfig(
                        "Hexagonal architecture requires slices configuration".to_string(),
                    ));
                }
            }
        }

        Ok(())
    }

    /// Get the absolute root path (relative to config file location)
    pub fn resolve_root(&self, config_dir: &Path) -> PathBuf {
        if self.root.is_absolute() {
            self.root.clone()
        } else {
            config_dir.join(&self.root)
        }
    }

    /// Get file extension for the configured language
    pub fn file_extension(&self) -> &str {
        match self.language.as_str() {
            "typescript" => "ts",
            "python" => "py",
            "rust" => "rs",
            _ => unreachable!("validated in validate()"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_v1() {
        let config = VsaConfig {
            version: 1,
            architecture: ArchitectureType::default(),
            root: PathBuf::from("./src/contexts"),
            language: "typescript".to_string(),
            domain: None,
            slices: None,
            infrastructure: None,
            framework: None,
            contexts: HashMap::new(),
            validation: ValidationConfig::default(),
            patterns: PatternsConfig::default(),
        };

        assert!(config.validate().is_ok());
        assert_eq!(config.file_extension(), "ts");
    }

    #[test]
    fn test_default_config_v2() {
        let config = VsaConfig {
            version: 2,
            architecture: ArchitectureType::HexagonalEventSourcedVsa,
            root: PathBuf::from("."),
            language: "typescript".to_string(),
            domain: Some(DomainConfig::default()),
            slices: Some(SlicesConfig::default()),
            infrastructure: Some(InfrastructureConfig::default()),
            framework: None,
            contexts: HashMap::new(),
            validation: ValidationConfig::default(),
            patterns: PatternsConfig::default(),
        };

        assert!(config.validate().is_ok());
        assert_eq!(config.file_extension(), "ts");
        assert_eq!(config.architecture, ArchitectureType::HexagonalEventSourcedVsa);
    }

    #[test]
    fn test_v2_requires_domain_config() {
        let config = VsaConfig {
            version: 2,
            architecture: ArchitectureType::HexagonalEventSourcedVsa,
            root: PathBuf::from("."),
            language: "typescript".to_string(),
            domain: None, // Missing domain config
            slices: Some(SlicesConfig::default()),
            infrastructure: None,
            framework: None,
            contexts: HashMap::new(),
            validation: ValidationConfig::default(),
            patterns: PatternsConfig::default(),
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn test_invalid_version() {
        let config = VsaConfig {
            version: 999,
            architecture: ArchitectureType::default(),
            root: PathBuf::from("./src"),
            language: "typescript".to_string(),
            domain: None,
            slices: None,
            infrastructure: None,
            framework: None,
            contexts: HashMap::new(),
            validation: ValidationConfig::default(),
            patterns: PatternsConfig::default(),
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn test_unsupported_language() {
        let config = VsaConfig {
            version: 1,
            architecture: ArchitectureType::default(),
            root: PathBuf::from("./src"),
            language: "java".to_string(),
            domain: None,
            slices: None,
            infrastructure: None,
            framework: None,
            contexts: HashMap::new(),
            validation: ValidationConfig::default(),
            patterns: PatternsConfig::default(),
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn test_event_versioning_config() {
        let event_config = EventConfig::default();
        assert!(event_config.versioning.enabled);
        assert_eq!(event_config.versioning.format, VersionFormat::Simple);
        assert!(event_config.versioning.require_decorator);
        assert!(event_config.versioning.require_upcasters);
    }

    #[test]
    fn test_slice_types() {
        let slices_config = SlicesConfig::default();
        assert_eq!(slices_config.types.len(), 3);
        assert!(slices_config.types.contains(&SliceType::Command));
        assert!(slices_config.types.contains(&SliceType::Query));
        assert!(slices_config.types.contains(&SliceType::Saga));
    }
}
