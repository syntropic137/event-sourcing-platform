//! Aggregate scanner
//!
//! Scans for aggregate files and extracts basic metadata.
//! Also detects entities and value objects within aggregate folders (aggregate_*/).

use crate::config::AggregateConfig;
use crate::domain::{Aggregate, AggregateEntity, AggregateValueObject};
use crate::error::Result;
use std::fs;
use std::path::Path;

/// Scanner for finding aggregates
pub struct AggregateScanner<'a> {
    #[allow(dead_code)]
    config: &'a AggregateConfig,
    root: &'a Path,
}

impl<'a> AggregateScanner<'a> {
    /// Create a new aggregate scanner
    pub fn new(config: &'a AggregateConfig, root: &'a Path) -> Self {
        Self { config, root }
    }

    /// Scan for aggregates
    ///
    /// This scanner looks for aggregates in two ways:
    /// 1. In `aggregate_*` folders (preferred DDD convention)
    /// 2. Loose *Aggregate.* files (legacy/simple projects)
    pub fn scan(&self) -> Result<Vec<Aggregate>> {
        let mut aggregates = Vec::new();

        self.scan_directory(self.root, &mut aggregates, false)?;

        Ok(aggregates)
    }

    /// Recursively scan a directory for aggregates
    ///
    /// `in_aggregate_folder` tracks if we're inside an aggregate_* folder
    fn scan_directory(
        &self,
        dir: &Path,
        aggregates: &mut Vec<Aggregate>,
        in_aggregate_folder: bool,
    ) -> Result<()> {
        if !dir.exists() || !dir.is_dir() {
            return Ok(());
        }

        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
                    // Skip hidden directories
                    if dir_name.starts_with('.') {
                        continue;
                    }

                    // Check if this is an aggregate_* folder
                    if dir_name.starts_with("aggregate_") {
                        // Scan the aggregate folder for its root, entities, and value objects
                        if let Some(aggregate) = self.scan_aggregate_folder(&path, dir_name)? {
                            aggregates.push(aggregate);
                        }
                    } else if !in_aggregate_folder {
                        // Continue scanning subdirectories (but not inside aggregate_* folders)
                        self.scan_directory(&path, aggregates, false)?;
                    }
                }
            } else if path.is_file() && !in_aggregate_folder {
                // Only scan for loose aggregate files if NOT inside an aggregate folder
                if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                    if self.matches_pattern(file_name) {
                        if let Some(aggregate) = self.parse_loose_aggregate(&path, file_name)? {
                            aggregates.push(aggregate);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Scan an aggregate_* folder for its root, entities, and value objects
    ///
    /// # DDD Convention
    /// - The folder name is `aggregate_<name>/` (e.g., `aggregate_workspace/`)
    /// - Contains exactly ONE `*Aggregate.*` file (the root)
    /// - Other files are entities or value objects
    fn scan_aggregate_folder(
        &self,
        folder_path: &Path,
        folder_name: &str,
    ) -> Result<Option<Aggregate>> {
        let mut root_file: Option<(String, std::path::PathBuf, usize)> = None;
        let mut entities = Vec::new();
        let mut value_objects = Vec::new();

        for entry in fs::read_dir(folder_path)? {
            let entry = entry?;
            let path = entry.path();

            if !path.is_file() {
                continue;
            }

            let file_name = match path.file_name().and_then(|n| n.to_str()) {
                Some(name) => name,
                None => continue,
            };

            // Skip non-code files and test files
            if !self.is_code_file(file_name) || self.is_test_file(file_name) {
                continue;
            }

            // Read file content
            let content = fs::read_to_string(&path)?;
            let line_count = content.lines().count();

            if self.matches_pattern(file_name) {
                // This is the aggregate root
                let name = self.extract_aggregate_name(file_name)?;
                root_file = Some((name, path.clone(), line_count));
            } else {
                // Classify as entity or value object based on content
                let class_name = self.extract_class_name(file_name);

                if self.is_entity(&content) {
                    let identity_field = self.detect_identity_field(&content);
                    entities.push(AggregateEntity {
                        name: class_name,
                        identity_field,
                        file_path: path,
                        line_count,
                    });
                } else if self.is_value_object(&content) {
                    let is_immutable = self.is_immutable(&content);
                    value_objects.push(AggregateValueObject {
                        name: class_name,
                        file_path: path,
                        is_immutable,
                        line_count,
                    });
                }
                // Note: Files that don't match entity or VO patterns are ignored
                // (e.g., __init__.py, helper files)
            }
        }

        // Only create aggregate if we found a root
        match root_file {
            Some((name, file_path, line_count)) => Ok(Some(Aggregate {
                name,
                context: None, // Will be set by domain scanner
                file_path,
                line_count,
                command_handlers: Vec::new(),
                event_handlers: Vec::new(),
                entities,
                value_objects,
                folder_name: Some(folder_name.to_string()),
            })),
            None => Ok(None), // No aggregate root found in this folder
        }
    }

    /// Parse a loose aggregate file (not in an aggregate_* folder)
    fn parse_loose_aggregate(
        &self,
        file_path: &Path,
        file_name: &str,
    ) -> Result<Option<Aggregate>> {
        let name = self.extract_aggregate_name(file_name)?;
        let content = fs::read_to_string(file_path)?;
        let line_count = content.lines().count();

        Ok(Some(Aggregate {
            name,
            context: None,
            file_path: file_path.to_path_buf(),
            line_count,
            command_handlers: Vec::new(),
            event_handlers: Vec::new(),
            entities: Vec::new(),
            value_objects: Vec::new(),
            folder_name: None, // Not in an aggregate folder
        }))
    }

    /// Check if a file name matches the aggregate pattern (*Aggregate.*)
    fn matches_pattern(&self, file_name: &str) -> bool {
        let name_without_ext = self.strip_extension(file_name);
        name_without_ext.ends_with("Aggregate")
    }

    /// Check if file is a code file
    fn is_code_file(&self, file_name: &str) -> bool {
        file_name.ends_with(".ts")
            || file_name.ends_with(".py")
            || file_name.ends_with(".rs")
            || file_name.ends_with(".tsx")
            || file_name.ends_with(".js")
    }

    /// Check if file is a test file
    fn is_test_file(&self, file_name: &str) -> bool {
        file_name.contains(".test.")
            || file_name.contains("_test.")
            || file_name.starts_with("test_")
            || file_name.contains(".spec.")
    }

    /// Strip file extension
    fn strip_extension<'b>(&self, file_name: &'b str) -> &'b str {
        file_name
            .strip_suffix(".ts")
            .or_else(|| file_name.strip_suffix(".tsx"))
            .or_else(|| file_name.strip_suffix(".py"))
            .or_else(|| file_name.strip_suffix(".rs"))
            .or_else(|| file_name.strip_suffix(".js"))
            .or_else(|| file_name.strip_suffix(".jsx"))
            .unwrap_or(file_name)
    }

    /// Extract aggregate name from file name
    fn extract_aggregate_name(&self, file_name: &str) -> Result<String> {
        Ok(self.strip_extension(file_name).to_string())
    }

    /// Extract class name from file name
    fn extract_class_name(&self, file_name: &str) -> String {
        self.strip_extension(file_name).to_string()
    }

    /// Check if content indicates an entity (has identity)
    ///
    /// Heuristics:
    /// - Has a field ending with `_id` (e.g., `isolation_id: str`)
    /// - Has an `id` field
    /// - Class name contains "Entity"
    fn is_entity(&self, content: &str) -> bool {
        // Look for identity fields
        let has_id_field = content.contains("_id:") || content.contains("_id =");
        let has_explicit_id = content.contains("id: ") || content.contains("id =");

        // Look for entity indicators
        let is_entity_class = content.contains("Entity") && !content.contains("ValueObject");

        has_id_field || has_explicit_id || is_entity_class
    }

    /// Check if content indicates a value object
    ///
    /// Heuristics:
    /// - frozen dataclass in Python
    /// - readonly in TypeScript
    /// - Class name contains "Policy", "Status", "Result"
    /// - No identity fields
    fn is_value_object(&self, content: &str) -> bool {
        // Check for frozen dataclass (Python)
        let is_frozen = content.contains("frozen=True") || content.contains("@frozen");

        // Check for readonly (TypeScript)
        let is_readonly = content.contains("readonly ");

        // Check for common value object name patterns
        let has_vo_name_pattern = content.contains("Policy")
            || content.contains("Status")
            || content.contains("Result")
            || content.contains("Specification")
            || content.contains("Money")
            || content.contains("Address");

        // If it's a dataclass but NOT an entity, it's likely a VO
        let is_dataclass_without_id = content.contains("@dataclass")
            && !content.contains("_id:")
            && !content.contains("_id =");

        is_frozen || is_readonly || has_vo_name_pattern || is_dataclass_without_id
    }

    /// Check if value object is immutable
    fn is_immutable(&self, content: &str) -> bool {
        content.contains("frozen=True")
            || content.contains("@frozen")
            || content.contains("readonly ")
            || content.contains("const ")
            || content.contains("immutable")
            || content.contains("Immutable")
    }

    /// Detect the identity field name from content
    fn detect_identity_field(&self, content: &str) -> Option<String> {
        // Look for patterns like "field_id: str" or "field_id ="
        for line in content.lines() {
            let line = line.trim();

            // Python pattern: field_name: type
            if line.contains("_id:") {
                if let Some(field) = line.split(':').next() {
                    let field = field.trim();
                    if field.ends_with("_id") && !field.starts_with('#') {
                        return Some(field.to_string());
                    }
                }
            }

            // TypeScript pattern: fieldName: type or readonly fieldName
            if line.contains("Id:") || line.contains("_id:") {
                if let Some(field) = line.split(':').next() {
                    let field = field.trim().trim_start_matches("readonly ");
                    if (field.ends_with("Id") || field.ends_with("_id")) && !field.starts_with("//")
                    {
                        return Some(field.to_string());
                    }
                }
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn create_test_config() -> AggregateConfig {
        AggregateConfig {
            path: PathBuf::from("."),
            pattern: "**/*Aggregate.*".to_string(),
            require_suffix: true,
            extensions: vec!["ts".to_string(), "py".to_string(), "rs".to_string()],
        }
    }

    #[test]
    fn test_scan_empty_directory() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let config = create_test_config();
        let scanner = AggregateScanner::new(&config, root);

        let aggregates = scanner.scan().unwrap();
        assert_eq!(aggregates.len(), 0);
    }

    #[test]
    fn test_scan_with_loose_aggregates() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create test aggregate files (loose, not in aggregate_* folder)
        fs::write(root.join("TaskAggregate.ts"), "// TaskAggregate\nclass TaskAggregate {}")
            .unwrap();
        fs::write(root.join("CartAggregate.ts"), "// CartAggregate\nclass CartAggregate {}")
            .unwrap();
        fs::write(root.join("SomeOtherFile.ts"), "// Just a file").unwrap();

        let config = create_test_config();
        let scanner = AggregateScanner::new(&config, root);

        let aggregates = scanner.scan().unwrap();

        assert_eq!(aggregates.len(), 2);

        let names: Vec<String> = aggregates.iter().map(|a| a.name.clone()).collect();
        assert!(names.contains(&"TaskAggregate".to_string()));
        assert!(names.contains(&"CartAggregate".to_string()));

        // Loose aggregates should not have folder_name
        for agg in &aggregates {
            assert!(agg.folder_name.is_none());
            assert!(agg.entities.is_empty());
            assert!(agg.value_objects.is_empty());
        }
    }

    #[test]
    fn test_scan_aggregate_folder() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create aggregate_workspace folder structure
        let agg_folder = root.join("aggregate_workspace");
        fs::create_dir_all(&agg_folder).unwrap();

        // Create aggregate root
        fs::write(agg_folder.join("WorkspaceAggregate.py"), "class WorkspaceAggregate:\n    pass")
            .unwrap();

        // Create entity with identity field
        fs::write(
            agg_folder.join("IsolationHandle.py"),
            "@dataclass\nclass IsolationHandle:\n    isolation_id: str\n    status: str",
        )
        .unwrap();

        // Create value object (frozen dataclass)
        fs::write(
            agg_folder.join("SecurityPolicy.py"),
            "@dataclass(frozen=True)\nclass SecurityPolicy:\n    level: str",
        )
        .unwrap();

        let config = create_test_config();
        let scanner = AggregateScanner::new(&config, root);

        let aggregates = scanner.scan().unwrap();

        assert_eq!(aggregates.len(), 1);

        let agg = &aggregates[0];
        assert_eq!(agg.name, "WorkspaceAggregate");
        assert_eq!(agg.folder_name, Some("aggregate_workspace".to_string()));
        assert_eq!(agg.entities.len(), 1);
        assert_eq!(agg.value_objects.len(), 1);

        // Check entity
        let entity = &agg.entities[0];
        assert_eq!(entity.name, "IsolationHandle");
        assert_eq!(entity.identity_field, Some("isolation_id".to_string()));

        // Check value object
        let vo = &agg.value_objects[0];
        assert_eq!(vo.name, "SecurityPolicy");
        assert!(vo.is_immutable);
    }

    #[test]
    fn test_scan_multiple_aggregate_folders() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create two aggregate folders
        let workspace_folder = root.join("aggregate_workspace");
        let workflow_folder = root.join("aggregate_workflow");
        fs::create_dir_all(&workspace_folder).unwrap();
        fs::create_dir_all(&workflow_folder).unwrap();

        fs::write(
            workspace_folder.join("WorkspaceAggregate.py"),
            "class WorkspaceAggregate:\n    pass",
        )
        .unwrap();
        fs::write(
            workflow_folder.join("WorkflowAggregate.py"),
            "class WorkflowAggregate:\n    pass",
        )
        .unwrap();

        let config = create_test_config();
        let scanner = AggregateScanner::new(&config, root);

        let aggregates = scanner.scan().unwrap();

        assert_eq!(aggregates.len(), 2);

        let names: Vec<String> = aggregates.iter().map(|a| a.name.clone()).collect();
        assert!(names.contains(&"WorkspaceAggregate".to_string()));
        assert!(names.contains(&"WorkflowAggregate".to_string()));
    }

    #[test]
    fn test_scan_nested_directories() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create nested structure with aggregate folders
        fs::create_dir_all(root.join("domain/aggregate_task")).unwrap();
        fs::create_dir_all(root.join("domain/aggregate_cart")).unwrap();

        fs::write(root.join("domain/aggregate_task/TaskAggregate.ts"), "class TaskAggregate {}")
            .unwrap();
        fs::write(root.join("domain/aggregate_cart/CartAggregate.ts"), "class CartAggregate {}")
            .unwrap();

        let config = create_test_config();
        let scanner = AggregateScanner::new(&config, root);

        let aggregates = scanner.scan().unwrap();
        assert_eq!(aggregates.len(), 2);

        // Both should have folder_name set
        for agg in &aggregates {
            assert!(agg.folder_name.is_some());
        }
    }

    #[test]
    fn test_extract_aggregate_name() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let config = create_test_config();
        let scanner = AggregateScanner::new(&config, root);

        assert_eq!(scanner.extract_aggregate_name("TaskAggregate.ts").unwrap(), "TaskAggregate");
        assert_eq!(scanner.extract_aggregate_name("CartAggregate.py").unwrap(), "CartAggregate");
        assert_eq!(scanner.extract_aggregate_name("OrderAggregate.rs").unwrap(), "OrderAggregate");
    }

    #[test]
    fn test_is_entity() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let config = create_test_config();
        let scanner = AggregateScanner::new(&config, root);

        // Entity with _id field
        assert!(scanner.is_entity("class Foo:\n    foo_id: str"));
        assert!(scanner.is_entity("@dataclass\nclass Bar:\n    bar_id = 'test'"));

        // Not an entity (no id)
        assert!(!scanner.is_entity("@dataclass(frozen=True)\nclass Money:\n    amount: int"));
    }

    #[test]
    fn test_is_value_object() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let config = create_test_config();
        let scanner = AggregateScanner::new(&config, root);

        // Frozen dataclass
        assert!(scanner.is_value_object("@dataclass(frozen=True)\nclass Money:\n    amount: int"));

        // Common VO name patterns
        assert!(scanner.is_value_object("class SecurityPolicy:\n    level: str"));
        assert!(scanner.is_value_object("class ExecutionStatus:\n    status: str"));
        assert!(scanner.is_value_object("class ExecutionResult:\n    code: int"));

        // Dataclass without id
        assert!(scanner.is_value_object("@dataclass\nclass Config:\n    setting: str"));
    }

    #[test]
    fn test_detect_identity_field() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let config = create_test_config();
        let scanner = AggregateScanner::new(&config, root);

        // Python style
        assert_eq!(
            scanner.detect_identity_field("class Foo:\n    isolation_id: str\n    other: int"),
            Some("isolation_id".to_string())
        );

        // TypeScript style
        assert_eq!(
            scanner.detect_identity_field("class Foo {\n  readonly phaseId: string;\n}"),
            Some("phaseId".to_string())
        );

        // No identity field
        assert_eq!(
            scanner.detect_identity_field("class Foo:\n    name: str\n    value: int"),
            None
        );
    }

    #[test]
    fn test_aggregate_folder_without_root() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create aggregate folder without a root aggregate file
        let agg_folder = root.join("aggregate_orphan");
        fs::create_dir_all(&agg_folder).unwrap();

        // Only entities, no aggregate root
        fs::write(
            agg_folder.join("SomeEntity.py"),
            "@dataclass\nclass SomeEntity:\n    entity_id: str",
        )
        .unwrap();

        let config = create_test_config();
        let scanner = AggregateScanner::new(&config, root);

        let aggregates = scanner.scan().unwrap();

        // Should not find any aggregates (no root found)
        assert_eq!(aggregates.len(), 0);
    }

    #[test]
    fn test_skips_test_files() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let agg_folder = root.join("aggregate_task");
        fs::create_dir_all(&agg_folder).unwrap();

        fs::write(agg_folder.join("TaskAggregate.py"), "class TaskAggregate:\n    pass").unwrap();
        fs::write(agg_folder.join("test_task.py"), "def test_task(): pass").unwrap();
        fs::write(agg_folder.join("TaskAggregate.test.ts"), "test('task', () => {})").unwrap();

        let config = create_test_config();
        let scanner = AggregateScanner::new(&config, root);

        let aggregates = scanner.scan().unwrap();

        assert_eq!(aggregates.len(), 1);
        // Test files should not be counted as entities or VOs
        assert!(aggregates[0].entities.is_empty());
        assert!(aggregates[0].value_objects.is_empty());
    }
}
