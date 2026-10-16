//! Domain layer scanner
//!
//! Scans the domain/ folder to extract metadata about all domain components.

use crate::config::{DomainConfig, EventVersioningConfig, FilenameConvention};
use crate::domain::{CommandHandler, DomainModel, Upcaster, ValueObject};
use crate::error::{Result, VsaError};
use crate::scanners::rust_source::{decision_arms, rust_code_only, SourceFile};
use crate::scanners::{
    AggregateScanner, CommandScanner, EventScanner, ProjectionScanner, QueryScanner,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

/// Scanner for the domain layer
pub struct DomainScanner {
    pub(crate) config: DomainConfig,
    pub(crate) root: PathBuf,
    filename_convention: FilenameConvention,
}

impl DomainScanner {
    /// Create a new domain scanner (pascal_case convention by default)
    pub fn new(config: DomainConfig, root: PathBuf) -> Self {
        Self { config, root, filename_convention: FilenameConvention::default() }
    }

    /// Set the filename convention threaded into the artifact sub-scanners so
    /// snake_case Rust contexts (`*_command.rs`, `*_event.rs`, ...) are detected.
    pub fn with_filename_convention(mut self, convention: FilenameConvention) -> Self {
        self.filename_convention = convention;
        self
    }

    /// Scan the domain folder and extract all metadata (monolithic mode)
    pub fn scan(&self) -> Result<DomainModel> {
        let domain_path = self.root.join(&self.config.path);
        self.scan_domain_at_path(&domain_path, None)
    }

    /// Scan a domain folder within a bounded context
    ///
    /// This method is used for multi-context architectures where each bounded context
    /// has its own domain folder. The context name is tagged on all discovered objects.
    ///
    /// # Arguments
    /// * `context_path` - Path to the bounded context directory
    /// * `context_name` - Name of the context (e.g., "workspaces", "artifacts")
    pub fn scan_context(&self, context_path: &Path, context_name: &str) -> Result<DomainModel> {
        let domain_path = context_path.join(&self.config.path);
        self.scan_domain_at_path(&domain_path, Some(context_name.to_string()))
    }

    /// Internal method to scan a domain folder at a specific path
    ///
    /// # Arguments
    /// * `domain_path` - Path to the domain folder
    /// * `context` - Optional context name to tag all objects with
    fn scan_domain_at_path(
        &self,
        domain_path: &Path,
        context: Option<String>,
    ) -> Result<DomainModel> {
        // Check if domain path exists
        if !domain_path.exists() {
            return Ok(DomainModel::new(domain_path.to_path_buf()));
        }

        if !domain_path.is_dir() {
            return Err(VsaError::IoError(std::io::Error::new(
                std::io::ErrorKind::NotADirectory,
                format!("Domain path is not a directory: {}", domain_path.display()),
            )));
        }

        let mut model = DomainModel::new(domain_path.to_path_buf());

        // Scan aggregates
        let aggregate_scanner = AggregateScanner::new(&self.config.aggregates, domain_path)
            .with_filename_convention(self.filename_convention.clone());
        model.aggregates = aggregate_scanner.scan()?;

        // Tag aggregates with context
        if let Some(ref ctx) = context {
            for aggregate in &mut model.aggregates {
                aggregate.context = Some(ctx.clone());
            }
        }

        // Scan commands
        let commands_path = domain_path.join(&self.config.commands.path);
        if commands_path.exists() {
            let command_scanner = CommandScanner::new(&self.config.commands, &commands_path)
                .with_filename_convention(self.filename_convention.clone());
            model.commands = command_scanner.scan()?;

            // Tag commands with context
            if let Some(ref ctx) = context {
                for command in &mut model.commands {
                    command.context = Some(ctx.clone());
                }
            }
        }

        // Scan queries
        let queries_path = domain_path.join(&self.config.queries.path);
        if queries_path.exists() {
            let query_scanner = QueryScanner::new(&self.config.queries, &queries_path)
                .with_filename_convention(self.filename_convention.clone());
            model.queries = query_scanner.scan()?;
            // Note: Queries don't have context field yet, may add in future
        }

        // Scan events
        let events_path = domain_path.join(&self.config.events.path);
        if events_path.exists() {
            let event_scanner = EventScanner::new(&self.config.events, &events_path)
                .with_filename_convention(self.filename_convention.clone());
            model.events = event_scanner.scan()?;

            // Tag events with context
            if let Some(ref ctx) = context {
                for event in &mut model.events {
                    event.context = Some(ctx.clone());
                }
            }
        }

        // Rust enums: which command variant each aggregate decides into which events.
        link_rust_decisions(domain_path, &mut model)?;

        // Scan upcasters
        if self.config.events.versioning.enabled {
            let upcasters_path = domain_path
                .join(&self.config.events.path)
                .join(&self.config.events.versioning.upcasters_path);

            if upcasters_path.exists() {
                model.upcasters =
                    self.scan_upcasters(&upcasters_path, &self.config.events.versioning)?;
            }
        }

        // Scan value objects
        model.value_objects = self.scan_value_objects(domain_path)?;

        // Scan projections
        // TODO: Consider restricting scan scope to slices/ or queries/ subtree to reduce
        // false positives and improve performance. Current implementation scans from context
        // root which can be overly broad for large codebases.
        let context_path = domain_path.parent().unwrap_or(domain_path);
        let projection_scanner = ProjectionScanner::new(None, context_path)
            .with_snake_case(self.filename_convention == FilenameConvention::SnakeCase);
        model.projections = projection_scanner.scan()?;

        // Tag projections with context
        if let Some(ref ctx) = context {
            for projection in &mut model.projections {
                projection.context = Some(ctx.clone());
            }
        }

        Ok(model)
    }

    /// Scan for upcasters in the _upcasters folder
    fn scan_upcasters(
        &self,
        upcasters_path: &Path,
        config: &EventVersioningConfig,
    ) -> Result<Vec<Upcaster>> {
        let mut upcasters = Vec::new();

        if !upcasters_path.exists() || !upcasters_path.is_dir() {
            return Ok(upcasters);
        }

        for entry in fs::read_dir(upcasters_path)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() {
                let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

                // Check if file matches upcaster pattern
                if self.matches_upcaster_pattern(file_name, &config.upcaster_pattern) {
                    if let Some(upcaster) = self.parse_upcaster(&path, file_name)? {
                        upcasters.push(upcaster);
                    }
                }
            }
        }

        Ok(upcasters)
    }

    /// Check if a file name matches the upcaster pattern
    fn matches_upcaster_pattern(&self, file_name: &str, _pattern: &str) -> bool {
        // Pattern examples:
        // "*_v*_to_v*.ts" -> "TaskCreated_v1_to_v2.ts"
        // "*_Upcaster_*.ts" -> "TaskCreated_Upcaster_V1_V2.ts"

        // Check for common upcaster patterns
        file_name.contains("_to_")
            || file_name.contains("Upcaster")
            || file_name.contains("upcaster")
    }

    /// Parse upcaster metadata from file name
    /// Expected format: "EventType_v1_to_v2.ts" or "EventType_Upcaster_v1_v2.ts"
    fn parse_upcaster(&self, file_path: &Path, file_name: &str) -> Result<Option<Upcaster>> {
        // Remove file extension
        let name_without_ext = file_name
            .strip_suffix(".ts")
            .or_else(|| file_name.strip_suffix(".py"))
            .or_else(|| file_name.strip_suffix(".rs"))
            .unwrap_or(file_name);

        // Try to parse format: "EventType_v1_to_v2"
        if let Some((event_and_from, to)) = name_without_ext.rsplit_once("_to_") {
            if let Some((event_type, from)) = event_and_from.rsplit_once('_') {
                return Ok(Some(Upcaster {
                    event_type: event_type.to_string(),
                    from_version: from.to_string(),
                    to_version: to.to_string(),
                    file_path: file_path.to_path_buf(),
                    decorator_present: false, // Will be set by AST parser in Milestone 4
                }));
            }
        }

        // Try to parse format: "EventType_Upcaster_v1_v2"
        if let Some((prefix, versions)) = name_without_ext.split_once("_Upcaster_") {
            let parts: Vec<&str> = versions.split('_').collect();
            if parts.len() == 2 {
                return Ok(Some(Upcaster {
                    event_type: prefix.to_string(),
                    from_version: parts[0].to_string(),
                    to_version: parts[1].to_string(),
                    file_path: file_path.to_path_buf(),
                    decorator_present: false,
                }));
            }
        }

        Ok(None)
    }

    /// Scan for value objects in the domain folder
    fn scan_value_objects(&self, domain_path: &Path) -> Result<Vec<ValueObject>> {
        let mut value_objects = Vec::new();

        if !domain_path.exists() || !domain_path.is_dir() {
            return Ok(value_objects);
        }

        // Scan for files matching *ValueObjects.* pattern
        for entry in fs::read_dir(domain_path)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() {
                let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

                // Check if file matches ValueObjects pattern
                if file_name.contains("ValueObjects") {
                    if let Some(value_object) = self.parse_value_object(&path, file_name)? {
                        value_objects.push(value_object);
                    }
                }
            }
        }

        Ok(value_objects)
    }

    /// Parse a value object file
    fn parse_value_object(&self, file_path: &Path, file_name: &str) -> Result<Option<ValueObject>> {
        // Extract value object name from file name
        let name = file_name.split('.').next().unwrap_or(file_name).to_string();

        // Read file content to count lines
        let content = fs::read_to_string(file_path)?;
        let line_count = content.lines().count();

        // Check for immutability indicators (basic heuristic)
        let is_immutable = content.contains("frozen=True")
            || content.contains("@dataclass(frozen=True)")
            || content.contains("readonly ")
            || content.contains("const ")
            || content.contains("immutable");

        Ok(Some(ValueObject {
            name,
            file_path: file_path.to_path_buf(),
            fields: Vec::new(), // Simple implementation - could be enhanced to parse fields
            is_immutable,
            line_count,
        }))
    }
}

/// Non-test `.rs` files under `dir` (recursively), as code with comments,
/// literals and test-only items blanked.
fn rust_code_under(dir: &Path) -> Result<Vec<(PathBuf, SourceFile)>> {
    let mut out = Vec::new();
    for entry in walkdir::WalkDir::new(dir).sort_by_file_name() {
        let entry = entry.map_err(std::io::Error::from)?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
        if !entry.file_type().is_file()
            || !name.ends_with(".rs")
            || name.ends_with("_test.rs")
            || name.ends_with("_tests.rs")
        {
            continue;
        }
        // A file's module is its stem; `mod.rs` is its folder's module.
        let module = match name {
            "mod.rs" => path.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str()),
            _ => name.strip_suffix(".rs"),
        }
        .unwrap_or_default()
        .to_string();
        let code = rust_code_only(&fs::read_to_string(path)?);
        out.push((path.to_path_buf(), SourceFile { module, code }));
    }
    Ok(out)
}

/// For a Rust domain whose commands and events are enum variants
/// (`Enum::Variant` names), record on each aggregate the command variants it
/// decides and the event variants each produces. The arms are read from the
/// files in the aggregate's own folder; the functions they call may live
/// anywhere under `domain_path`. A command decided in several arms (a guarded
/// arm and its fallback) gets the union of their events. Nothing changes for
/// other languages.
fn link_rust_decisions(domain_path: &Path, model: &mut DomainModel) -> Result<()> {
    let variants_by_enum = |names: Vec<&String>| {
        let mut map: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for name in names {
            if let Some((enum_name, variant)) = name.split_once("::") {
                map.entry(enum_name.to_string()).or_default().insert(variant.to_string());
            }
        }
        map
    };
    let commands = variants_by_enum(model.commands.iter().map(|c| &c.name).collect());
    let events = variants_by_enum(model.events.iter().map(|e| &e.name).collect());
    if commands.is_empty() || events.is_empty() {
        return Ok(());
    }

    let domain_code = rust_code_under(domain_path)?;
    for aggregate in &mut model.aggregates {
        if !aggregate.command_handlers.is_empty() {
            continue;
        }
        let Some(folder) = aggregate.file_path.parent() else { continue };
        let (arm_files, helper_files): (Vec<_>, Vec<_>) =
            domain_code.iter().partition(|(path, _)| path.parent() == Some(folder));
        let arm_files: Vec<SourceFile> = arm_files.into_iter().map(|(_, f)| f.clone()).collect();
        let helper_files: Vec<SourceFile> =
            helper_files.into_iter().map(|(_, f)| f.clone()).collect();
        for arm in decision_arms(&arm_files, &helper_files, &commands, &events) {
            match aggregate.command_handlers.iter_mut().find(|h| h.command_type == arm.command) {
                Some(handler) => {
                    for event in arm.events {
                        if !handler.emits_events.contains(&event) {
                            handler.emits_events.push(event);
                        }
                    }
                    handler.emits_events.sort();
                }
                None => aggregate.command_handlers.push(CommandHandler {
                    command_type: arm.command,
                    method_name: arm.function,
                    line_number: arm.line,
                    emits_events: arm.events,
                }),
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AggregateConfig, CommandConfig, EventConfig, QueryConfig};
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn create_test_domain_config() -> DomainConfig {
        DomainConfig {
            path: PathBuf::from("domain"),
            aggregates: AggregateConfig {
                path: PathBuf::from("."),
                pattern: "**/*Aggregate.*".to_string(),
                require_suffix: true,
                extensions: vec!["ts".to_string(), "py".to_string(), "rs".to_string()],
            },
            commands: CommandConfig {
                path: PathBuf::from("commands"),
                pattern: "**/*Command.*".to_string(),
                require_suffix: true,
                require_aggregate_id: true,
                extensions: vec!["ts".to_string(), "py".to_string(), "rs".to_string()],
                organize_by_feature: false,
            },
            queries: QueryConfig {
                path: PathBuf::from("queries"),
                pattern: "**/*Query.*".to_string(),
                require_suffix: true,
                extensions: vec!["ts".to_string(), "py".to_string(), "rs".to_string()],
                organize_by_feature: false,
            },
            events: EventConfig {
                path: PathBuf::from("events"),
                pattern: "**/*Event.*".to_string(),
                require_suffix: true,
                extensions: vec!["ts".to_string(), "py".to_string(), "rs".to_string()],
                versioning: EventVersioningConfig {
                    enabled: true,
                    format: crate::config::VersionFormat::Simple,
                    require_decorator: true,
                    require_upcasters: true,
                    versioned_path: PathBuf::from("_versioned"),
                    upcasters_path: PathBuf::from("_upcasters"),
                    upcaster_pattern: "*_v*_to_v*.ts".to_string(),
                },
            },
        }
    }

    #[test]
    fn test_scan_empty_domain() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        let config = create_test_domain_config();
        let scanner = DomainScanner::new(config, root);

        let model = scanner.scan().unwrap();
        assert_eq!(model.component_count(), 0);
    }

    #[test]
    fn test_scan_nonexistent_domain() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        let config = create_test_domain_config();
        let scanner = DomainScanner::new(config, root);

        // Should return empty model, not error
        let model = scanner.scan().unwrap();
        assert_eq!(model.component_count(), 0);
    }

    #[test]
    fn test_matches_upcaster_pattern() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        let config = create_test_domain_config();
        let scanner = DomainScanner::new(config, root);

        assert!(scanner.matches_upcaster_pattern("TaskCreated_v1_to_v2.ts", "*_v*_to_v*.ts"));
        assert!(
            scanner.matches_upcaster_pattern("TaskCreated_Upcaster_v1_v2.ts", "*_Upcaster_*.ts")
        );
        assert!(!scanner.matches_upcaster_pattern("TaskCreatedEvent.ts", "*_v*_to_v*.ts"));
    }

    #[test]
    fn test_parse_upcaster_standard_format() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        let config = create_test_domain_config();
        let scanner = DomainScanner::new(config, root.clone());

        let file_path = root.join("TaskCreated_v1_to_v2.ts");
        let upcaster =
            scanner.parse_upcaster(&file_path, "TaskCreated_v1_to_v2.ts").unwrap().unwrap();

        assert_eq!(upcaster.event_type, "TaskCreated");
        assert_eq!(upcaster.from_version, "v1");
        assert_eq!(upcaster.to_version, "v2");
    }

    #[test]
    fn test_parse_upcaster_class_format() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        let config = create_test_domain_config();
        let scanner = DomainScanner::new(config, root.clone());

        let file_path = root.join("TaskCreated_Upcaster_v1_v2.ts");
        let upcaster =
            scanner.parse_upcaster(&file_path, "TaskCreated_Upcaster_v1_v2.ts").unwrap().unwrap();

        assert_eq!(upcaster.event_type, "TaskCreated");
        assert_eq!(upcaster.from_version, "v1");
        assert_eq!(upcaster.to_version, "v2");
    }

    #[test]
    fn test_scan_with_domain_structure() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();
        let domain_path = root.join("domain");

        // Create domain structure
        fs::create_dir_all(&domain_path).unwrap();
        fs::create_dir_all(domain_path.join("commands")).unwrap();
        fs::create_dir_all(domain_path.join("queries")).unwrap();
        fs::create_dir_all(domain_path.join("events")).unwrap();
        fs::create_dir_all(domain_path.join("events/_upcasters")).unwrap();

        // Create test files
        fs::write(domain_path.join("TaskAggregate.ts"), "// TaskAggregate").unwrap();
        fs::write(domain_path.join("commands/CreateTaskCommand.ts"), "// CreateTaskCommand")
            .unwrap();
        fs::write(domain_path.join("queries/GetTaskQuery.ts"), "// GetTaskQuery").unwrap();
        fs::write(domain_path.join("events/TaskCreatedEvent.ts"), "// TaskCreatedEvent").unwrap();
        fs::write(domain_path.join("events/_upcasters/TaskCreated_v1_to_v2.ts"), "// Upcaster")
            .unwrap();

        let config = create_test_domain_config();
        let scanner = DomainScanner::new(config, root);

        let model = scanner.scan().unwrap();

        // Should find at least some components (exact counts depend on scanners)
        assert!(
            !model.aggregates.is_empty()
                || !model.commands.is_empty()
                || !model.queries.is_empty()
                || !model.events.is_empty()
                || !model.upcasters.is_empty()
        );
    }

    #[test]
    fn test_scan_context_tags_objects() {
        let temp_dir = TempDir::new().unwrap();
        let context_path = temp_dir.path().join("contexts/workspaces");
        let domain_path = context_path.join("domain");

        // Create domain structure within context
        fs::create_dir_all(&domain_path).unwrap();
        fs::create_dir_all(domain_path.join("commands")).unwrap();
        fs::create_dir_all(domain_path.join("events")).unwrap();

        // Create test files
        fs::write(domain_path.join("WorkspaceAggregate.py"), "# WorkspaceAggregate").unwrap();
        fs::write(
            domain_path.join("commands/CreateWorkspaceCommand.py"),
            "# CreateWorkspaceCommand",
        )
        .unwrap();
        fs::write(domain_path.join("events/WorkspaceCreatedEvent.py"), "# WorkspaceCreatedEvent")
            .unwrap();

        let config = create_test_domain_config();
        let scanner = DomainScanner::new(config, temp_dir.path().to_path_buf());

        let model = scanner.scan_context(&context_path, "workspaces").unwrap();

        // Verify aggregates are tagged with context
        assert!(!model.aggregates.is_empty());
        for aggregate in &model.aggregates {
            assert_eq!(aggregate.context, Some("workspaces".to_string()));
        }

        // Verify commands are tagged with context
        assert!(!model.commands.is_empty());
        for command in &model.commands {
            assert_eq!(command.context, Some("workspaces".to_string()));
        }

        // Verify events are tagged with context
        assert!(!model.events.is_empty());
        for event in &model.events {
            assert_eq!(event.context, Some("workspaces".to_string()));
        }
    }

    #[test]
    fn test_scan_context_nonexistent_domain() {
        let temp_dir = TempDir::new().unwrap();
        let context_path = temp_dir.path().join("contexts/orders");

        let config = create_test_domain_config();
        let scanner = DomainScanner::new(config, temp_dir.path().to_path_buf());

        // Should return empty model, not error
        let model = scanner.scan_context(&context_path, "orders").unwrap();
        assert_eq!(model.component_count(), 0);
    }

    #[test]
    fn test_scan_monolithic_no_context_tags() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();
        let domain_path = root.join("domain");

        // Create domain structure
        fs::create_dir_all(&domain_path).unwrap();
        fs::write(domain_path.join("TaskAggregate.ts"), "// TaskAggregate").unwrap();

        let config = create_test_domain_config();
        let scanner = DomainScanner::new(config, root);

        let model = scanner.scan().unwrap();

        // Monolithic scan should NOT tag with context
        if !model.aggregates.is_empty() {
            assert_eq!(model.aggregates[0].context, None);
        }
    }
}
