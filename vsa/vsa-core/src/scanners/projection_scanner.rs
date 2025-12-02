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

    /// Check if a file name matches the projection pattern (*Projection.*)
    fn matches_pattern(&self, file_name: &str) -> bool {
        // Remove extension first
        let name_without_ext = file_name
            .strip_suffix(".ts")
            .or_else(|| file_name.strip_suffix(".tsx"))
            .or_else(|| file_name.strip_suffix(".py"))
            .or_else(|| file_name.strip_suffix(".rs"))
            .unwrap_or(file_name);

        // Check if it ends with "Projection"
        name_without_ext.ends_with("Projection")
    }

    /// Parse projection metadata from a file
    fn parse_projection(&self, file_path: &Path, file_name: &str) -> Result<Option<Projection>> {
        // Extract projection name from file name
        let name = self.extract_projection_name(file_name)?;

        // Read file content to extract subscribed events
        let content = fs::read_to_string(file_path)?;
        let line_count = content.lines().count();

        // Extract subscribed events from content
        let subscribed_events = self.extract_subscribed_events(&content);

        // Extract read model from content
        let read_model = self.extract_read_model(&content);

        Ok(Some(Projection {
            name,
            file_path: file_path.to_path_buf(),
            subscribed_events,
            read_model,
            line_count,
        }))
    }

    /// Extract projection name from file name
    fn extract_projection_name(&self, file_name: &str) -> Result<String> {
        // Remove file extension
        let name_without_ext = file_name
            .strip_suffix(".ts")
            .or_else(|| file_name.strip_suffix(".tsx"))
            .or_else(|| file_name.strip_suffix(".py"))
            .or_else(|| file_name.strip_suffix(".rs"))
            .unwrap_or(file_name);

        Ok(name_without_ext.to_string())
    }

    /// Extract subscribed events from file content
    ///
    /// Looks for patterns like:
    /// - TypeScript: `on_WorkflowCreated`, `handle(event: WorkflowCreatedEvent)`
    /// - Python: `async def on_WorkflowCreated`, `def handle_WorkflowCreated`
    /// - Decorators: `@Handles(WorkflowCreatedEvent)`, `@Subscribe(WorkflowCreatedEvent)`
    fn extract_subscribed_events(&self, content: &str) -> Vec<String> {
        let mut events = Vec::new();

        // Pattern 1: on_<EventName> method (Python/TS)
        // Matches: on_WorkflowCreated, on_WorkflowCompleted
        let on_pattern = Regex::new(r"(?:async\s+)?def\s+on_(\w+)|on_(\w+)\s*[:\(]").unwrap();
        for cap in on_pattern.captures_iter(content) {
            if let Some(event_name) = cap.get(1).or(cap.get(2)) {
                let name = event_name.as_str();
                // Skip common non-event methods
                if !["init", "error", "complete", "start", "end"]
                    .contains(&name.to_lowercase().as_str())
                {
                    events.push(format!("{name}Event"));
                }
            }
        }

        // Pattern 2: handle_<EventName> method (Python)
        let handle_pattern = Regex::new(r"def\s+handle_(\w+Event)").unwrap();
        for cap in handle_pattern.captures_iter(content) {
            if let Some(event_name) = cap.get(1) {
                events.push(event_name.as_str().to_string());
            }
        }

        // Pattern 3: @Handles or @Subscribe decorator (TS/Python)
        let decorator_pattern =
            Regex::new(r"@(?:Handles|Subscribe|EventHandler)\s*\(\s*(\w+Event)\s*\)").unwrap();
        for cap in decorator_pattern.captures_iter(content) {
            if let Some(event_name) = cap.get(1) {
                let name = event_name.as_str().to_string();
                if !events.contains(&name) {
                    events.push(name);
                }
            }
        }

        // Pattern 4: event type annotation (TS)
        // Matches: handle(event: WorkflowCreatedEvent)
        let type_pattern = Regex::new(r"handle\s*\(\s*\w+:\s*(\w+Event)\s*\)").unwrap();
        for cap in type_pattern.captures_iter(content) {
            if let Some(event_name) = cap.get(1) {
                let name = event_name.as_str().to_string();
                if !events.contains(&name) {
                    events.push(name);
                }
            }
        }

        // Pattern 5: subscribed_events list (Python)
        // Matches: subscribed_events = ["WorkflowCreatedEvent", ...]
        let list_pattern = Regex::new(r#"subscribed_events\s*=\s*\[([^\]]+)\]"#).unwrap();
        if let Some(cap) = list_pattern.captures(content) {
            if let Some(list_content) = cap.get(1) {
                let event_list_pattern = Regex::new(r#"["'](\w+Event)["']"#).unwrap();
                for event_cap in event_list_pattern.captures_iter(list_content.as_str()) {
                    if let Some(event_name) = event_cap.get(1) {
                        let name = event_name.as_str().to_string();
                        if !events.contains(&name) {
                            events.push(name);
                        }
                    }
                }
            }
        }

        events
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

        assert_eq!(
            scanner.extract_projection_name("WorkflowListProjection.py").unwrap(),
            "WorkflowListProjection"
        );
        assert_eq!(
            scanner.extract_projection_name("CartItemsProjection.ts").unwrap(),
            "CartItemsProjection"
        );
    }

    #[test]
    fn test_matches_pattern() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let config = create_test_config();
        let scanner = ProjectionScanner::new(Some(&config), root);

        assert!(scanner.matches_pattern("WorkflowListProjection.py"));
        assert!(scanner.matches_pattern("CartItemsProjection.ts"));
        assert!(scanner.matches_pattern("OrderProjection.rs"));
        assert!(!scanner.matches_pattern("WorkflowHandler.py"));
        assert!(!scanner.matches_pattern("WorkflowQuery.ts"));
        assert!(!scanner.matches_pattern("projection.py")); // lowercase doesn't match
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
}
