//! Projection scanner
//!
//! Scans for projection files and extracts metadata including subscribed events.
//! Projections follow the pattern *Projection.* and are typically found in query slices.

use crate::config::QuerySliceConfig;
use crate::domain::Projection;
use crate::error::Result;
use regex::Regex;
use std::fs;
use std::path::Path;

/// Scanner for finding projections in query slices
pub struct ProjectionScanner<'a> {
    #[allow(dead_code)]
    config: Option<&'a QuerySliceConfig>,
    root: &'a Path,
}

impl<'a> ProjectionScanner<'a> {
    /// Create a new projection scanner
    pub fn new(config: Option<&'a QuerySliceConfig>, root: &'a Path) -> Self {
        Self { config, root }
    }

    /// Scan for projections
    pub fn scan(&self) -> Result<Vec<Projection>> {
        let mut projections = Vec::new();

        self.scan_directory(self.root, &mut projections)?;

        Ok(projections)
    }

    /// Recursively scan a directory for projections
    fn scan_directory(&self, dir: &Path, projections: &mut Vec<Projection>) -> Result<()> {
        if !dir.exists() || !dir.is_dir() {
            return Ok(());
        }

        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                // Skip hidden directories and common non-source directories
                if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
                    if !dir_name.starts_with('.')
                        && dir_name != "node_modules"
                        && dir_name != "__pycache__"
                        && dir_name != "target"
                    {
                        self.scan_directory(&path, projections)?;
                    }
                }
            } else if path.is_file() {
                if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                    if self.matches_pattern(file_name) {
                        if let Some(projection) = self.parse_projection(&path, file_name)? {
                            projections.push(projection);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Check if a file name matches the projection pattern (*Projection.* or projection.*)
    fn matches_pattern(&self, file_name: &str) -> bool {
        // Remove extension first
        let name_without_ext = file_name
            .strip_suffix(".ts")
            .or_else(|| file_name.strip_suffix(".tsx"))
            .or_else(|| file_name.strip_suffix(".py"))
            .or_else(|| file_name.strip_suffix(".rs"))
            .unwrap_or(file_name);

        // Check if it ends with "Projection" (e.g., WorkflowListProjection.py)
        // OR if it's exactly named "projection" (e.g., projection.py)
        name_without_ext.ends_with("Projection") || name_without_ext == "projection"
    }

    /// Parse projection metadata from a file
    fn parse_projection(&self, file_path: &Path, file_name: &str) -> Result<Option<Projection>> {
        // Read file content to extract projection class name and metadata
        let content = fs::read_to_string(file_path)?;
        let line_count = content.lines().count();

        // Extract projection name from file name or class name in content
        let name = self.extract_projection_name_from_content(&content, file_name)?;

        // Extract subscribed events from content
        let subscribed_events = self.extract_subscribed_events(&content);

        // Extract read model from content
        let read_model = self.extract_read_model(&content);

        Ok(Some(Projection {
            name,
            file_path: file_path.to_path_buf(),
            subscribed_events,
            read_model,
            context: None, // Will be set by DomainScanner if in a context
            line_count,
        }))
    }

    /// Extract projection name from file content or file name
    ///
    /// For files named generically (e.g., projection.py), extract class name from content.
    /// For files named specifically (e.g., WorkflowListProjection.py), use file name.
    fn extract_projection_name_from_content(
        &self,
        content: &str,
        file_name: &str,
    ) -> Result<String> {
        // Remove file extension
        let name_without_ext = file_name
            .strip_suffix(".ts")
            .or_else(|| file_name.strip_suffix(".tsx"))
            .or_else(|| file_name.strip_suffix(".py"))
            .or_else(|| file_name.strip_suffix(".rs"))
            .unwrap_or(file_name);

        // If file is generically named (e.g., "projection"), extract class name from content
        if name_without_ext == "projection" {
            // Look for class definition ending in "Projection"
            // Matches: class WorkflowListProjection, export class OrderProjection
            let class_pattern = Regex::new(r"(?:class|export\s+class)\s+(\w+Projection)").unwrap();
            if let Some(cap) = class_pattern.captures(content) {
                if let Some(class_name) = cap.get(1) {
                    return Ok(class_name.as_str().to_string());
                }
            }

            // Fallback: use "Projection" if no class found
            return Ok("Projection".to_string());
        }

        // Otherwise use file name
        Ok(name_without_ext.to_string())
    }

    /// Extract subscribed events from file content
    ///
    /// Looks for patterns like:
    /// - TypeScript: `on_WorkflowCreated`, `handle(event: WorkflowCreatedEvent)`
    /// - Python: `async def on_WorkflowCreated`, `def handle_WorkflowCreated`
    /// - Python (snake_case): `async def on_session_started` → `session_started`
    /// - Decorators: `@Handles(WorkflowCreatedEvent)`, `@Subscribe(WorkflowCreatedEvent)`
    fn extract_subscribed_events(&self, content: &str) -> Vec<String> {
        use std::collections::HashSet;
        let mut events = HashSet::new();

        // Method verb prefixes that indicate CRUD/accessor methods, not events
        // e.g., get_by_id, set_value, load_data are methods, not events
        const METHOD_VERB_PREFIXES: &[&str] = &[
            "get", "set", "load", "save", "create", "update", "delete",
        ];

        // Non-event words for PascalCase validation (single-word exclusions)
        // These are excluded when they appear as standalone PascalCase names
        const NON_EVENT_METHODS: &[&str] = &[
            "init", "error", "complete", "start", "end",
            "id", "ms", "type", "data", "time", "name", "value",
            "projection", "event", "message", "result", "status",
            "create", "update", "delete", "get", "set", "load", "save",
        ];

        // Helper to check if snake_case name looks like a valid event
        fn is_valid_snake_case_event(name: &str) -> bool {
            // Must consist of multiple snake_case components (e.g., session_started)
            let parts: Vec<&str> = name.split('_').filter(|s| !s.is_empty()).collect();

            // Must have at least 2 parts (e.g., session_started)
            if parts.len() < 2 {
                return false;
            }

            // Each part must be at least 2 chars to avoid noise like "a_b"
            if parts.iter().any(|p| p.len() < 2) {
                return false;
            }

            // Exclude if first word is a method verb prefix (e.g., get_by_id, set_value)
            // These are clearly accessor/CRUD methods, not events
            let first_word = parts.first().unwrap_or(&"");
            if METHOD_VERB_PREFIXES.contains(first_word) {
                return false;
            }

            // Allow noun prefixes like error_occurred, data_received, status_changed
            // These are valid event names even though the words appear in NON_EVENT_METHODS
            true
        }

        // Helper to check if PascalCase name looks like a valid event
        fn is_valid_pascal_case_event(name: &str) -> bool {
            // Must start with uppercase
            // AND be longer than 3 chars
            // AND not be in the exclusion list
            name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
                && name.len() > 3
                && !NON_EVENT_METHODS.contains(&name.to_lowercase().as_str())
        }

        // Pattern 1: on_<EventName> method (Python/TS)
        // Supports both conventions:
        // - PascalCase: on_WorkflowCreated → WorkflowCreatedEvent
        // - snake_case: on_session_started → session_started
        // Uses word boundary (\b) to avoid matching inside words like "execution_and_phase"
        let on_pattern = Regex::new(r"(?:async\s+)?def\s+on_(\w+)|\bon_(\w+)\s*[:\(]").unwrap();
        for cap in on_pattern.captures_iter(content) {
            if let Some(event_name) = cap.get(1).or(cap.get(2)) {
                let name = event_name.as_str();

                // Check if it's snake_case (contains underscore)
                if name.contains('_') && is_valid_snake_case_event(name) {
                    // snake_case events: on_session_started → session_started
                    events.insert(name.to_string());
                } else if is_valid_pascal_case_event(name) {
                    // PascalCase events: on_WorkflowCreated → WorkflowCreatedEvent
                    events.insert(format!("{name}Event"));
                }
            }
        }

        // Pattern 2: handle_<EventName> method (Python)
        // Supports both conventions:
        // - PascalCase: handle_WorkflowCreatedEvent → WorkflowCreatedEvent
        // - snake_case: handle_session_started → session_started
        let handle_pattern = Regex::new(r"def\s+handle_(\w+)").unwrap();
        for cap in handle_pattern.captures_iter(content) {
            if let Some(event_name) = cap.get(1) {
                let name = event_name.as_str();

                // If it already ends with Event (PascalCase), use as-is
                if name.ends_with("Event") {
                    events.insert(name.to_string());
                } else if name.contains('_') && is_valid_snake_case_event(name) {
                    // snake_case events: handle_session_started → session_started
                    events.insert(name.to_string());
                } else if is_valid_pascal_case_event(name) {
                    // PascalCase without Event suffix: handle_WorkflowCreated → WorkflowCreatedEvent
                    events.insert(format!("{name}Event"));
                }
            }
        }

        // Pattern 3: @Handles or @Subscribe decorator (TS/Python)
        let decorator_pattern =
            Regex::new(r"@(?:Handles|Subscribe|EventHandler)\s*\(\s*(\w+Event)\s*\)").unwrap();
        for cap in decorator_pattern.captures_iter(content) {
            if let Some(event_name) = cap.get(1) {
                events.insert(event_name.as_str().to_string());
            }
        }

        // Pattern 4: event type annotation (TS)
        // Matches: handle(event: WorkflowCreatedEvent)
        let type_pattern = Regex::new(r"handle\s*\(\s*\w+:\s*(\w+Event)\s*\)").unwrap();
        for cap in type_pattern.captures_iter(content) {
            if let Some(event_name) = cap.get(1) {
                events.insert(event_name.as_str().to_string());
            }
        }

        // Pattern 5: subscribed_events list (Python)
        // Matches both conventions:
        // - PascalCase: subscribed_events = ["WorkflowCreatedEvent", ...]
        // - snake_case: subscribed_events = ["session_started", "session_completed", ...]
        let list_pattern = Regex::new(r#"subscribed_events\s*=\s*\[([^\]]+)\]"#).unwrap();
        if let Some(cap) = list_pattern.captures(content) {
            if let Some(list_content) = cap.get(1) {
                // Match any quoted string in the list
                let event_list_pattern = Regex::new(r#"["'](\w+)["']"#).unwrap();
                for event_cap in event_list_pattern.captures_iter(list_content.as_str()) {
                    if let Some(event_name) = event_cap.get(1) {
                        let name = event_name.as_str();

                        // PascalCase events ending with Event
                        if name.ends_with("Event") {
                            events.insert(name.to_string());
                        } else if name.contains('_') && is_valid_snake_case_event(name) {
                            // snake_case events
                            events.insert(name.to_string());
                        }
                    }
                }
            }
        }

        // Convert to Vec and sort for deterministic output
        let mut result: Vec<String> = events.into_iter().collect();
        result.sort();
        result
    }

    /// Extract read model type from file content
    fn extract_read_model(&self, content: &str) -> Option<String> {
        // Pattern 1: class name ending in ReadModel
        let read_model_pattern =
            Regex::new(r"(?:class|interface|type)\s+(\w+(?:ReadModel|Summary|Detail|View))")
                .unwrap();
        if let Some(cap) = read_model_pattern.captures(content) {
            return cap.get(1).map(|m| m.as_str().to_string());
        }

        // Pattern 2: Projection generic type (TS)
        // Matches: Projection<WorkflowSummary>
        let generic_pattern = Regex::new(r"Projection\s*<\s*(\w+)\s*>").unwrap();
        if let Some(cap) = generic_pattern.captures(content) {
            return cap.get(1).map(|m| m.as_str().to_string());
        }

        // Pattern 3: read_model type annotation (Python)
        let annotation_pattern = Regex::new(r"read_model:\s*(?:Type\[)?(\w+)").unwrap();
        if let Some(cap) = annotation_pattern.captures(content) {
            return cap.get(1).map(|m| m.as_str().to_string());
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_config() -> QuerySliceConfig {
        QuerySliceConfig {
            pattern: "*".to_string(),
            require_projection: true,
            must_use: "QueryBus".to_string(),
            max_lines: Some(100),
            require_tests: true,
            adapters: vec!["rest".to_string()],
        }
    }

    #[test]
    fn test_scan_empty_directory() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let config = create_test_config();
        let scanner = ProjectionScanner::new(Some(&config), root);

        let projections = scanner.scan().unwrap();
        assert_eq!(projections.len(), 0);
    }

    #[test]
    fn test_scan_with_projections() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create test projection files
        fs::write(
            root.join("WorkflowListProjection.py"),
            r#"
from event_sourcing import Projection

class WorkflowListProjection(Projection):
    async def on_WorkflowCreated(self, event):
        pass

    async def on_WorkflowCompleted(self, event):
        pass
"#,
        )
        .unwrap();

        let config = create_test_config();
        let scanner = ProjectionScanner::new(Some(&config), root);

        let projections = scanner.scan().unwrap();
        assert_eq!(projections.len(), 1);
        assert_eq!(projections[0].name, "WorkflowListProjection");
        assert!(projections[0].subscribed_events.contains(&"WorkflowCreatedEvent".to_string()));
        assert!(projections[0].subscribed_events.contains(&"WorkflowCompletedEvent".to_string()));
    }

    #[test]
    fn test_scan_typescript_projection() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        fs::write(
            root.join("CartItemsProjection.ts"),
            r#"
import { Projection } from '@event-sourcing/core';
import { ItemAddedEvent, ItemRemovedEvent } from '../domain/events';

export class CartItemsProjection extends Projection<CartItemsReadModel> {
    @Handles(ItemAddedEvent)
    onItemAdded(event: ItemAddedEvent): void {
        // handle event
    }

    @Handles(ItemRemovedEvent)
    onItemRemoved(event: ItemRemovedEvent): void {
        // handle event
    }
}
"#,
        )
        .unwrap();

        let config = create_test_config();
        let scanner = ProjectionScanner::new(Some(&config), root);

        let projections = scanner.scan().unwrap();
        assert_eq!(projections.len(), 1);
        assert_eq!(projections[0].name, "CartItemsProjection");
        assert!(projections[0].subscribed_events.contains(&"ItemAddedEvent".to_string()));
        assert!(projections[0].subscribed_events.contains(&"ItemRemovedEvent".to_string()));
        assert_eq!(projections[0].read_model, Some("CartItemsReadModel".to_string()));
    }

    #[test]
    fn test_scan_nested_directories() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create nested slice structure
        fs::create_dir_all(root.join("slices/list_workflows")).unwrap();
        fs::create_dir_all(root.join("slices/get_workflow_detail")).unwrap();

        fs::write(
            root.join("slices/list_workflows/WorkflowListProjection.py"),
            r#"
class WorkflowListProjection:
    subscribed_events = ["WorkflowCreatedEvent"]
"#,
        )
        .unwrap();

        fs::write(
            root.join("slices/get_workflow_detail/WorkflowDetailProjection.py"),
            r#"
class WorkflowDetailProjection:
    def on_WorkflowCreated(self, event): pass
    def on_PhaseStarted(self, event): pass
"#,
        )
        .unwrap();

        let config = create_test_config();
        let scanner = ProjectionScanner::new(Some(&config), root);

        let projections = scanner.scan().unwrap();
        assert_eq!(projections.len(), 2);

        let names: Vec<&str> = projections.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"WorkflowListProjection"));
        assert!(names.contains(&"WorkflowDetailProjection"));
    }

    #[test]
    fn test_extract_projection_name() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let config = create_test_config();
        let scanner = ProjectionScanner::new(Some(&config), root);

        // Test with specifically named files (use file name)
        assert_eq!(
            scanner.extract_projection_name_from_content("", "WorkflowListProjection.py").unwrap(),
            "WorkflowListProjection"
        );
        assert_eq!(
            scanner.extract_projection_name_from_content("", "CartItemsProjection.ts").unwrap(),
            "CartItemsProjection"
        );

        // Test with generically named file (extract from content)
        let py_content = "class WorkflowDetailProjection:\n    pass";
        assert_eq!(
            scanner.extract_projection_name_from_content(py_content, "projection.py").unwrap(),
            "WorkflowDetailProjection"
        );

        let ts_content = "export class OrderListProjection {\n}";
        assert_eq!(
            scanner.extract_projection_name_from_content(ts_content, "projection.ts").unwrap(),
            "OrderListProjection"
        );
    }

    #[test]
    fn test_matches_pattern() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let config = create_test_config();
        let scanner = ProjectionScanner::new(Some(&config), root);

        // Specifically named projections
        assert!(scanner.matches_pattern("WorkflowListProjection.py"));
        assert!(scanner.matches_pattern("CartItemsProjection.ts"));
        assert!(scanner.matches_pattern("OrderProjection.rs"));

        // Generically named projections (NEW)
        assert!(scanner.matches_pattern("projection.py"));
        assert!(scanner.matches_pattern("projection.ts"));
        assert!(scanner.matches_pattern("projection.rs"));

        // Non-projection files
        assert!(!scanner.matches_pattern("WorkflowHandler.py"));
        assert!(!scanner.matches_pattern("WorkflowQuery.ts"));
    }

    #[test]
    fn test_extract_subscribed_events_python() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let config = create_test_config();
        let scanner = ProjectionScanner::new(Some(&config), root);

        let content = r#"
class TestProjection:
    async def on_WorkflowCreated(self, event):
        pass

    async def on_WorkflowCompleted(self, event):
        pass
"#;

        let events = scanner.extract_subscribed_events(content);
        assert!(events.contains(&"WorkflowCreatedEvent".to_string()));
        assert!(events.contains(&"WorkflowCompletedEvent".to_string()));
    }

    #[test]
    fn test_extract_subscribed_events_list() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let config = create_test_config();
        let scanner = ProjectionScanner::new(Some(&config), root);

        let content = r#"
class TestProjection:
    subscribed_events = ["WorkflowCreatedEvent", "SessionStartedEvent"]
"#;

        let events = scanner.extract_subscribed_events(content);
        assert!(events.contains(&"WorkflowCreatedEvent".to_string()));
        assert!(events.contains(&"SessionStartedEvent".to_string()));
    }

    #[test]
    fn test_extract_read_model() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let config = create_test_config();
        let scanner = ProjectionScanner::new(Some(&config), root);

        let ts_content = r#"
export class WorkflowProjection extends Projection<WorkflowSummary> {
}
"#;
        assert_eq!(scanner.extract_read_model(ts_content), Some("WorkflowSummary".to_string()));

        let py_content = r#"
class WorkflowReadModel:
    pass
"#;
        assert_eq!(scanner.extract_read_model(py_content), Some("WorkflowReadModel".to_string()));
    }

    #[test]
    fn test_no_false_positive_from_mid_word_on_pattern() {
        // Regression test: Pattern 1 should NOT match "on_" in the middle of words
        // like "get_by_execution_and_phase" extracting "and_phase"
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let config = create_test_config();
        let scanner = ProjectionScanner::new(Some(&config), root);

        let content = r#"
class ArtifactListProjection:
    async def on_artifact_created(self, event_data: dict) -> None:
        pass

    async def get_by_execution_and_phase(
        self,
        execution_id: str,
        phase_id: str,
    ) -> list:
        pass
"#;

        let events = scanner.extract_subscribed_events(content);
        // Should extract artifact_created
        assert!(events.contains(&"artifact_created".to_string()));
        // Should NOT extract and_phase (false positive from mid-word match)
        assert!(!events.contains(&"and_phase".to_string()));
        assert!(!events.contains(&"and_phaseEvent".to_string()));
    }

    #[test]
    fn test_snake_case_events_with_common_words_allowed() {
        // Events like error_occurred, data_received should be valid
        // even though "error" and "data" are in NON_EVENT_METHODS
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let config = create_test_config();
        let scanner = ProjectionScanner::new(Some(&config), root);

        let content = r#"
class ErrorHandlingProjection:
    async def on_error_occurred(self, event_data: dict) -> None:
        pass

    async def on_data_received(self, event_data: dict) -> None:
        pass

    async def on_status_changed(self, event_data: dict) -> None:
        pass
"#;

        let events = scanner.extract_subscribed_events(content);
        // Multi-word snake_case events should be extracted even if first word is common noun
        assert!(events.contains(&"error_occurred".to_string()));
        assert!(events.contains(&"data_received".to_string()));
        assert!(events.contains(&"status_changed".to_string()));
    }

    #[test]
    fn test_method_verb_prefixes_excluded() {
        // Method patterns like get_by_id, set_value, load_data should NOT be extracted
        // These are clearly CRUD/accessor methods, not events
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let config = create_test_config();
        let scanner = ProjectionScanner::new(Some(&config), root);

        let content = r#"
class DataProjection:
    async def on_get_by_id(self, id: str) -> None:
        pass

    async def on_set_value(self, value: str) -> None:
        pass

    async def on_load_data(self, data: dict) -> None:
        pass

    async def on_save_changes(self) -> None:
        pass

    async def on_create_item(self, item: dict) -> None:
        pass

    async def on_session_started(self, event_data: dict) -> None:
        pass
"#;

        let events = scanner.extract_subscribed_events(content);
        // Method verb prefixes should NOT be extracted as events
        assert!(!events.contains(&"get_by_id".to_string()));
        assert!(!events.contains(&"set_value".to_string()));
        assert!(!events.contains(&"load_data".to_string()));
        assert!(!events.contains(&"save_changes".to_string()));
        assert!(!events.contains(&"create_item".to_string()));
        // But valid events should still be extracted
        assert!(events.contains(&"session_started".to_string()));
    }

    #[test]
    fn test_pattern2_handle_snake_case_events() {
        // Pattern 2: handle_<event> should support snake_case events
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let config = create_test_config();
        let scanner = ProjectionScanner::new(Some(&config), root);

        let content = r#"
class GitHubProjection:
    def handle_session_started(self, event_data: dict) -> None:
        pass

    def handle_WorkflowCreatedEvent(self, event: WorkflowCreatedEvent) -> None:
        pass

    def handle_OrderPlaced(self, event: OrderPlacedEvent) -> None:
        pass
"#;

        let events = scanner.extract_subscribed_events(content);
        // snake_case: handle_session_started → session_started
        assert!(events.contains(&"session_started".to_string()));
        // PascalCase with Event suffix: handle_WorkflowCreatedEvent → WorkflowCreatedEvent
        assert!(events.contains(&"WorkflowCreatedEvent".to_string()));
        // PascalCase without Event suffix: handle_OrderPlaced → OrderPlacedEvent
        assert!(events.contains(&"OrderPlacedEvent".to_string()));
    }

    #[test]
    fn test_pattern5_subscribed_events_list_snake_case() {
        // Pattern 5: subscribed_events list should support snake_case events
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let config = create_test_config();
        let scanner = ProjectionScanner::new(Some(&config), root);

        let content = r#"
class SessionListProjection:
    subscribed_events = ["session_started", "session_completed", "WorkflowCreatedEvent"]
"#;

        let events = scanner.extract_subscribed_events(content);
        // snake_case events from list
        assert!(events.contains(&"session_started".to_string()));
        assert!(events.contains(&"session_completed".to_string()));
        // PascalCase events from list
        assert!(events.contains(&"WorkflowCreatedEvent".to_string()));
    }

    #[test]
    fn test_subscribed_events_list_ignores_non_events() {
        // Pattern 5: should ignore non-event strings in subscribed_events list
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let config = create_test_config();
        let scanner = ProjectionScanner::new(Some(&config), root);

        let content = r#"
class TestProjection:
    subscribed_events = ["session_started", "id", "ms", "WorkflowCreatedEvent"]
"#;

        let events = scanner.extract_subscribed_events(content);
        // Valid events should be extracted
        assert!(events.contains(&"session_started".to_string()));
        assert!(events.contains(&"WorkflowCreatedEvent".to_string()));
        // Single-word non-event strings should be ignored
        assert!(!events.contains(&"id".to_string()));
        assert!(!events.contains(&"ms".to_string()));
        assert!(!events.contains(&"idEvent".to_string()));
        assert!(!events.contains(&"msEvent".to_string()));
    }
}
