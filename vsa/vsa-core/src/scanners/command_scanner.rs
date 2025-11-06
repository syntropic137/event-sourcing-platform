//! Command scanner
//!
//! Scans for command files and extracts basic metadata.

use crate::config::CommandConfig;
use crate::domain::Command;
use crate::error::Result;
use std::fs;
use std::path::Path;

/// Scanner for finding commands
pub struct CommandScanner<'a> {
    #[allow(dead_code)]
    config: &'a CommandConfig,
    root: &'a Path,
}

impl<'a> CommandScanner<'a> {
    /// Create a new command scanner
    pub fn new(config: &'a CommandConfig, root: &'a Path) -> Self {
        Self { config, root }
    }

    /// Scan for commands
    pub fn scan(&self) -> Result<Vec<Command>> {
        let mut commands = Vec::new();

        self.scan_directory(self.root, &mut commands)?;

        Ok(commands)
    }

    /// Recursively scan a directory for commands
    fn scan_directory(&self, dir: &Path, commands: &mut Vec<Command>) -> Result<()> {
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
                        self.scan_directory(&path, commands)?;
                    }
                }
            } else if path.is_file() {
                if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                    if self.matches_pattern(file_name) {
                        if let Some(command) = self.parse_command(&path, file_name)? {
                            commands.push(command);
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

        // Check if it ends with "Command"
        name_without_ext.ends_with("Command")
    }

    /// Parse command metadata from a file
    fn parse_command(&self, file_path: &Path, file_name: &str) -> Result<Option<Command>> {
        // Extract command name from file name
        let name = self.extract_command_name(file_name)?;

        // Read file content to check for aggregateId
        let content = fs::read_to_string(file_path)?;
        let has_aggregate_id = content.contains("aggregateId") || content.contains("aggregate_id");

        // For now, we create a basic command without fields
        // Fields will be populated by AST parser in Milestone 4
        Ok(Some(Command {
            name,
            file_path: file_path.to_path_buf(),
            has_aggregate_id,
            fields: Vec::new(),
        }))
    }

    /// Extract command name from file name
    fn extract_command_name(&self, file_name: &str) -> Result<String> {
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

    fn create_test_config() -> CommandConfig {
        CommandConfig {
            path: PathBuf::from("commands"),
            pattern: "**/*Command.*".to_string(),
            require_suffix: true,
            require_aggregate_id: true,
            extensions: vec!["ts".to_string(), "py".to_string(), "rs".to_string()],
            organize_by_feature: false,
        }
    }

    #[test]
    fn test_scan_empty_directory() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let config = create_test_config();
        let scanner = CommandScanner::new(&config, root);

        let commands = scanner.scan().unwrap();
        assert_eq!(commands.len(), 0);
    }

    #[test]
    fn test_scan_with_commands() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create test command files
        fs::write(
            root.join("CreateTaskCommand.ts"),
            "export class CreateTaskCommand { aggregateId: string; }",
        )
        .unwrap();
        fs::write(
            root.join("CompleteTaskCommand.ts"),
            "export class CompleteTaskCommand { aggregateId: string; }",
        )
        .unwrap();

        let config = create_test_config();
        let scanner = CommandScanner::new(&config, root);

        let commands = scanner.scan().unwrap();
        assert_eq!(commands.len(), 2);

        let names: Vec<String> = commands.iter().map(|c| c.name.clone()).collect();
        assert!(names.contains(&"CreateTaskCommand".to_string()));
        assert!(names.contains(&"CompleteTaskCommand".to_string()));
    }

    #[test]
    fn test_scan_nested_directories() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create nested structure (feature-based organization)
        fs::create_dir_all(root.join("create-task")).unwrap();
        fs::create_dir_all(root.join("complete-task")).unwrap();

        fs::write(
            root.join("create-task/CreateTaskCommand.ts"),
            "export class CreateTaskCommand { aggregateId: string; }",
        )
        .unwrap();
        fs::write(
            root.join("complete-task/CompleteTaskCommand.ts"),
            "export class CompleteTaskCommand { aggregateId: string; }",
        )
        .unwrap();

        let config = create_test_config();
        let scanner = CommandScanner::new(&config, root);

        let commands = scanner.scan().unwrap();
        assert_eq!(commands.len(), 2);
    }

    #[test]
    fn test_extract_command_name() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let config = create_test_config();
        let scanner = CommandScanner::new(&config, root);

        assert_eq!(
            scanner.extract_command_name("CreateTaskCommand.ts").unwrap(),
            "CreateTaskCommand"
        );
        assert_eq!(
            scanner.extract_command_name("CompleteTaskCommand.py").unwrap(),
            "CompleteTaskCommand"
        );
    }

    #[test]
    fn test_parse_command_with_aggregate_id() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();
        let file_path = root.join("CreateTaskCommand.ts");

        fs::write(
            &file_path,
            "export class CreateTaskCommand { aggregateId: string; title: string; }",
        )
        .unwrap();

        let config = create_test_config();
        let scanner = CommandScanner::new(&config, root);

        let command = scanner.parse_command(&file_path, "CreateTaskCommand.ts").unwrap().unwrap();

        assert_eq!(command.name, "CreateTaskCommand");
        assert!(command.has_aggregate_id);
    }

    #[test]
    fn test_parse_command_without_aggregate_id() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();
        let file_path = root.join("SomeCommand.ts");

        fs::write(&file_path, "export class SomeCommand { title: string; }").unwrap();

        let config = create_test_config();
        let scanner = CommandScanner::new(&config, root);

        let command = scanner.parse_command(&file_path, "SomeCommand.ts").unwrap().unwrap();

        assert_eq!(command.name, "SomeCommand");
        assert!(!command.has_aggregate_id);
    }
}
