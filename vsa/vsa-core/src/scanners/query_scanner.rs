//! Query scanner
//!
//! Scans for query files and extracts basic metadata.

use crate::config::QueryConfig;
use crate::domain::Query;
use crate::error::Result;
use std::fs;
use std::path::Path;

/// Scanner for finding queries
pub struct QueryScanner<'a> {
    #[allow(dead_code)]
    config: &'a QueryConfig,
    root: &'a Path,
}

impl<'a> QueryScanner<'a> {
    /// Create a new query scanner
    pub fn new(config: &'a QueryConfig, root: &'a Path) -> Self {
        Self { config, root }
    }

    /// Scan for queries
    pub fn scan(&self) -> Result<Vec<Query>> {
        let mut queries = Vec::new();

        self.scan_directory(self.root, &mut queries)?;

        Ok(queries)
    }

    /// Recursively scan a directory for queries
    fn scan_directory(&self, dir: &Path, queries: &mut Vec<Query>) -> Result<()> {
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
                        self.scan_directory(&path, queries)?;
                    }
                }
            } else if path.is_file() {
                if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                    if self.matches_pattern(file_name) {
                        if let Some(query) = self.parse_query(&path, file_name)? {
                            queries.push(query);
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

        // Check if it ends with "Query"
        name_without_ext.ends_with("Query")
    }

    /// Parse query metadata from a file
    fn parse_query(&self, file_path: &Path, file_name: &str) -> Result<Option<Query>> {
        // Extract query name from file name
        let name = self.extract_query_name(file_name)?;

        // For now, we create a basic query without fields
        // Fields will be populated by AST parser in Milestone 4
        Ok(Some(Query { name, file_path: file_path.to_path_buf(), fields: Vec::new() }))
    }

    /// Extract query name from file name
    fn extract_query_name(&self, file_name: &str) -> Result<String> {
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

    fn create_test_config() -> QueryConfig {
        QueryConfig {
            path: PathBuf::from("queries"),
            pattern: "**/*Query.*".to_string(),
            require_suffix: true,
            extensions: vec!["ts".to_string(), "py".to_string(), "rs".to_string()],
            organize_by_feature: false,
        }
    }

    #[test]
    fn test_scan_empty_directory() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let config = create_test_config();
        let scanner = QueryScanner::new(&config, root);

        let queries = scanner.scan().unwrap();
        assert_eq!(queries.len(), 0);
    }

    #[test]
    fn test_scan_with_queries() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create test query files
        fs::write(
            root.join("GetTaskByIdQuery.ts"),
            "export class GetTaskByIdQuery { taskId: string; }",
        )
        .unwrap();
        fs::write(root.join("ListTasksQuery.ts"), "export class ListTasksQuery { }").unwrap();

        let config = create_test_config();
        let scanner = QueryScanner::new(&config, root);

        let queries = scanner.scan().unwrap();
        assert_eq!(queries.len(), 2);

        let names: Vec<String> = queries.iter().map(|q| q.name.clone()).collect();
        assert!(names.contains(&"GetTaskByIdQuery".to_string()));
        assert!(names.contains(&"ListTasksQuery".to_string()));
    }

    #[test]
    fn test_scan_nested_directories() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create nested structure
        fs::create_dir_all(root.join("tasks")).unwrap();
        fs::create_dir_all(root.join("cart")).unwrap();

        fs::write(
            root.join("tasks/GetTaskByIdQuery.ts"),
            "export class GetTaskByIdQuery { taskId: string; }",
        )
        .unwrap();
        fs::write(
            root.join("cart/GetCartQuery.ts"),
            "export class GetCartQuery { cartId: string; }",
        )
        .unwrap();

        let config = create_test_config();
        let scanner = QueryScanner::new(&config, root);

        let queries = scanner.scan().unwrap();
        assert_eq!(queries.len(), 2);
    }

    #[test]
    fn test_extract_query_name() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let config = create_test_config();
        let scanner = QueryScanner::new(&config, root);

        assert_eq!(scanner.extract_query_name("GetTaskByIdQuery.ts").unwrap(), "GetTaskByIdQuery");
        assert_eq!(scanner.extract_query_name("ListTasksQuery.py").unwrap(), "ListTasksQuery");
    }

    #[test]
    fn test_parse_query() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();
        let file_path = root.join("GetTaskByIdQuery.ts");

        fs::write(&file_path, "export class GetTaskByIdQuery { taskId: string; }").unwrap();

        let config = create_test_config();
        let scanner = QueryScanner::new(&config, root);

        let query = scanner.parse_query(&file_path, "GetTaskByIdQuery.ts").unwrap().unwrap();

        assert_eq!(query.name, "GetTaskByIdQuery");
        assert!(query.is_get_by_id_query());
    }
}
