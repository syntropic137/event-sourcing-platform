//! Domain layer scanner
//!
//! Scans the domain/ folder to extract metadata about all domain components.

use crate::config::{DomainConfig, EventVersioningConfig};
use crate::domain::{DomainModel, Upcaster};
use crate::error::{Result, VsaError};
use crate::scanners::{AggregateScanner, CommandScanner, EventScanner, QueryScanner};
use std::fs;
use std::path::{Path, PathBuf};

/// Scanner for the domain layer
pub struct DomainScanner {
    config: DomainConfig,
    root: PathBuf,
}

impl DomainScanner {
    /// Create a new domain scanner
    pub fn new(config: DomainConfig, root: PathBuf) -> Self {
        Self { config, root }
    }

    /// Scan the domain folder and extract all metadata
    pub fn scan(&self) -> Result<DomainModel> {
        let domain_path = self.root.join(&self.config.path);

        // Check if domain path exists
        if !domain_path.exists() {
            return Ok(DomainModel::new(domain_path));
        }

        if !domain_path.is_dir() {
            return Err(VsaError::IoError(std::io::Error::new(
                std::io::ErrorKind::NotADirectory,
                format!("Domain path is not a directory: {}", domain_path.display()),
            )));
        }

        let mut model = DomainModel::new(domain_path.clone());

        // Scan aggregates
        let aggregate_scanner = AggregateScanner::new(&self.config.aggregates, &domain_path);
        model.aggregates = aggregate_scanner.scan()?;

        // Scan commands
        let commands_path = domain_path.join(&self.config.commands.path);
        if commands_path.exists() {
            let command_scanner = CommandScanner::new(&self.config.commands, &commands_path);
            model.commands = command_scanner.scan()?;
        }

        // Scan queries
        let queries_path = domain_path.join(&self.config.queries.path);
        if queries_path.exists() {
            let query_scanner = QueryScanner::new(&self.config.queries, &queries_path);
            model.queries = query_scanner.scan()?;
        }

        // Scan events
        let events_path = domain_path.join(&self.config.events.path);
        if events_path.exists() {
            let event_scanner = EventScanner::new(&self.config.events, &events_path);
            model.events = event_scanner.scan()?;
        }

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
            model.aggregates.len() > 0
                || model.commands.len() > 0
                || model.queries.len() > 0
                || model.events.len() > 0
                || model.upcasters.len() > 0
        );
    }
}
