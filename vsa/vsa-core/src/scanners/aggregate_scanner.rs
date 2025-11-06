//! Aggregate scanner
//!
//! Scans for aggregate files and extracts basic metadata.

use crate::config::AggregateConfig;
use crate::domain::Aggregate;
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
    pub fn scan(&self) -> Result<Vec<Aggregate>> {
        let mut aggregates = Vec::new();

        self.scan_directory(self.root, &mut aggregates)?;

        Ok(aggregates)
    }

    /// Recursively scan a directory for aggregates
    fn scan_directory(&self, dir: &Path, aggregates: &mut Vec<Aggregate>) -> Result<()> {
        if !dir.exists() || !dir.is_dir() {
            return Ok(());
        }

        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                // Skip hidden directories
                if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
                    if !dir_name.starts_with('.') {
                        self.scan_directory(&path, aggregates)?;
                    }
                }
            } else if path.is_file() {
                if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                    if self.matches_pattern(file_name) {
                        if let Some(aggregate) = self.parse_aggregate(&path, file_name)? {
                            aggregates.push(aggregate);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Check if a file name matches the pattern
    fn matches_pattern(&self, file_name: &str) -> bool {
        // Remove extension first
        let name_without_ext = file_name
            .strip_suffix(".ts")
            .or_else(|| file_name.strip_suffix(".py"))
            .or_else(|| file_name.strip_suffix(".rs"))
            .unwrap_or(file_name);

        // Check if it ends with "Aggregate"
        name_without_ext.ends_with("Aggregate")
    }

    /// Parse aggregate metadata from a file
    fn parse_aggregate(&self, file_path: &Path, file_name: &str) -> Result<Option<Aggregate>> {
        // Extract aggregate name from file name
        let name = self.extract_aggregate_name(file_name)?;

        // Read file to get line count
        let content = fs::read_to_string(file_path)?;
        let line_count = content.lines().count();

        // For now, we create a basic aggregate without handlers
        // Handlers will be populated by AST parser in Milestone 4
        Ok(Some(Aggregate {
            name,
            file_path: file_path.to_path_buf(),
            line_count,
            command_handlers: Vec::new(),
            event_handlers: Vec::new(),
        }))
    }

    /// Extract aggregate name from file name
    fn extract_aggregate_name(&self, file_name: &str) -> Result<String> {
        // Remove file extension
        let name_without_ext = file_name
            .strip_suffix(".ts")
            .or_else(|| file_name.strip_suffix(".py"))
            .or_else(|| file_name.strip_suffix(".rs"))
            .unwrap_or(file_name);

        Ok(name_without_ext.to_string())
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
    fn test_scan_with_aggregates() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create test aggregate files
        fs::write(root.join("TaskAggregate.ts"), "// TaskAggregate\nclass TaskAggregate {}")
            .unwrap();
        fs::write(root.join("CartAggregate.ts"), "// CartAggregate\nclass CartAggregate {}")
            .unwrap();
        fs::write(root.join("SomeOtherFile.ts"), "// Just a file").unwrap(); // Won't match pattern

        let config = create_test_config();
        let scanner = AggregateScanner::new(&config, root);

        let aggregates = scanner.scan().unwrap();

        // Should find 2 aggregates
        assert_eq!(aggregates.len(), 2);

        let names: Vec<String> = aggregates.iter().map(|a| a.name.clone()).collect();
        assert!(names.contains(&"TaskAggregate".to_string()));
        assert!(names.contains(&"CartAggregate".to_string()));
    }

    #[test]
    fn test_scan_nested_directories() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create nested structure
        fs::create_dir_all(root.join("tasks")).unwrap();
        fs::create_dir_all(root.join("cart")).unwrap();

        fs::write(root.join("tasks/TaskAggregate.ts"), "// TaskAggregate").unwrap();
        fs::write(root.join("cart/CartAggregate.ts"), "// CartAggregate").unwrap();

        let config = create_test_config();
        let scanner = AggregateScanner::new(&config, root);

        let aggregates = scanner.scan().unwrap();
        assert_eq!(aggregates.len(), 2);
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
    fn test_parse_aggregate() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();
        let file_path = root.join("TaskAggregate.ts");

        // Create a test file with multiple lines
        fs::write(&file_path, "// TaskAggregate\nclass TaskAggregate {\n  // Some content\n}")
            .unwrap();

        let config = create_test_config();
        let scanner = AggregateScanner::new(&config, root);

        let aggregate = scanner.parse_aggregate(&file_path, "TaskAggregate.ts").unwrap().unwrap();

        assert_eq!(aggregate.name, "TaskAggregate");
        assert_eq!(aggregate.line_count, 4);
        assert_eq!(aggregate.command_handlers.len(), 0); // Will be populated by AST parser
        assert_eq!(aggregate.event_handlers.len(), 0);
    }
}
