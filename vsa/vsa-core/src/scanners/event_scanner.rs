//! Event scanner
//!
//! Scans for event files and extracts basic metadata including versions.

use crate::config::EventConfig;
use crate::domain::{Event, EventVersion};
use crate::error::Result;
use std::fs;
use std::path::Path;

/// Scanner for finding events
pub struct EventScanner<'a> {
    config: &'a EventConfig,
    root: &'a Path,
}

impl<'a> EventScanner<'a> {
    /// Create a new event scanner
    pub fn new(config: &'a EventConfig, root: &'a Path) -> Self {
        Self { config, root }
    }

    /// Scan for events
    pub fn scan(&self) -> Result<Vec<Event>> {
        let mut events = Vec::new();

        // Scan main events directory
        self.scan_directory(self.root, &mut events, false)?;

        // Scan versioned events directory if versioning is enabled
        if self.config.versioning.enabled {
            let versioned_path = self.root.join(&self.config.versioning.versioned_path);
            if versioned_path.exists() {
                self.scan_directory(&versioned_path, &mut events, true)?;
            }
        }

        Ok(events)
    }

    /// Recursively scan a directory for events
    fn scan_directory(
        &self,
        dir: &Path,
        events: &mut Vec<Event>,
        is_versioned: bool,
    ) -> Result<()> {
        if !dir.exists() || !dir.is_dir() {
            return Ok(());
        }

        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                // Skip hidden directories and special folders
                if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
                    if !dir_name.starts_with('.')
                        && dir_name != "_upcasters"
                        && dir_name
                            != self.config.versioning.versioned_path.to_string_lossy().as_ref()
                    {
                        self.scan_directory(&path, events, is_versioned)?;
                    }
                }
            } else if path.is_file() {
                if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                    if self.matches_pattern(file_name) {
                        if let Some(event) = self.parse_event(&path, file_name, is_versioned)? {
                            events.push(event);
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

        // Remove version suffix if present (e.g., ".v2" or ".2.1.0")
        let base_name = if let Some(idx) = name_without_ext.rfind('.') {
            &name_without_ext[..idx]
        } else {
            name_without_ext
        };

        // Check if it ends with "Event"
        base_name.ends_with("Event")
    }

    /// Parse event metadata from a file
    fn parse_event(
        &self,
        file_path: &Path,
        file_name: &str,
        is_versioned: bool,
    ) -> Result<Option<Event>> {
        // Extract event name and version from file name
        let (name, event_type, version) = self.extract_event_info(file_name, is_versioned)?;

        // For now, we create a basic event without fields
        // Fields will be populated by AST parser in Milestone 4
        Ok(Some(Event {
            name,
            event_type,
            version,
            file_path: file_path.to_path_buf(),
            fields: Vec::new(),
            decorator_present: false, // Will be set by AST parser
        }))
    }

    /// Extract event name, type, and version from file name
    ///
    /// Examples:
    /// - "TaskCreatedEvent.ts" -> ("TaskCreatedEvent", "TaskCreated", "v1")
    /// - "TaskCreatedEvent.v2.ts" -> ("TaskCreatedEvent", "TaskCreated", "v2")
    /// - "ItemAddedEvent.2.0.0.ts" -> ("ItemAddedEvent", "ItemAdded", "2.0.0")
    fn extract_event_info(
        &self,
        file_name: &str,
        is_versioned: bool,
    ) -> Result<(String, String, EventVersion)> {
        // Remove file extension
        let name_without_ext = file_name
            .strip_suffix(".ts")
            .or_else(|| file_name.strip_suffix(".py"))
            .or_else(|| file_name.strip_suffix(".rs"))
            .unwrap_or(file_name);

        // Check for version in file name (e.g., "TaskCreatedEvent.v2" or "TaskCreatedEvent.2.0.0")
        // We need to handle multiple dots for semver (e.g., "TaskCreatedEvent.2.1.0")
        let (base_name, version) = self.parse_name_and_version(name_without_ext, is_versioned);

        // Extract event type (remove "Event" suffix if present)
        let event_type = if base_name.ends_with("Event") {
            base_name.strip_suffix("Event").unwrap_or(&base_name).to_string()
        } else {
            base_name.clone()
        };

        Ok((base_name, event_type, version))
    }

    /// Parse event name and version from a string
    fn parse_name_and_version(&self, name: &str, _is_versioned: bool) -> (String, EventVersion) {
        // Try to find version patterns
        // First try semver (e.g., "TaskCreatedEvent.2.1.0")
        let parts: Vec<&str> = name.split('.').collect();

        if parts.len() == 4 {
            // Potential semver: name.major.minor.patch
            if let (Ok(major), Ok(minor), Ok(patch)) =
                (parts[1].parse::<u32>(), parts[2].parse::<u32>(), parts[3].parse::<u32>())
            {
                return (parts[0].to_string(), EventVersion::Semver(major, minor, patch));
            }
        }

        if parts.len() == 2 {
            // Could be simple version (e.g., "TaskCreatedEvent.v2")
            if let Some(version) = EventVersion::parse(parts[1]) {
                return (parts[0].to_string(), version);
            }
        }

        // No version found - default to v1
        let version = EventVersion::Simple("v1".to_string());

        (name.to_string(), version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{EventVersioningConfig, VersionFormat};
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn create_test_config() -> EventConfig {
        EventConfig {
            path: PathBuf::from("events"),
            pattern: "**/*Event.*".to_string(),
            require_suffix: true,
            extensions: vec!["ts".to_string(), "py".to_string(), "rs".to_string()],
            versioning: EventVersioningConfig {
                enabled: true,
                format: VersionFormat::Simple,
                require_decorator: true,
                require_upcasters: true,
                versioned_path: PathBuf::from("_versioned"),
                upcasters_path: PathBuf::from("_upcasters"),
                upcaster_pattern: "*_v*_to_v*.ts".to_string(),
            },
        }
    }

    #[test]
    fn test_scan_empty_directory() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let config = create_test_config();
        let scanner = EventScanner::new(&config, root);

        let events = scanner.scan().unwrap();
        assert_eq!(events.len(), 0);
    }

    #[test]
    fn test_scan_with_events() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create test event files
        fs::write(root.join("TaskCreatedEvent.ts"), "export class TaskCreatedEvent { }").unwrap();
        fs::write(root.join("TaskCompletedEvent.ts"), "export class TaskCompletedEvent { }")
            .unwrap();

        let config = create_test_config();
        let scanner = EventScanner::new(&config, root);

        let events = scanner.scan().unwrap();
        assert_eq!(events.len(), 2);

        let names: Vec<String> = events.iter().map(|e| e.name.clone()).collect();
        assert!(names.contains(&"TaskCreatedEvent".to_string()));
        assert!(names.contains(&"TaskCompletedEvent".to_string()));
    }

    #[test]
    fn test_scan_with_versioned_events() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create current version
        fs::write(root.join("TaskCreatedEvent.ts"), "export class TaskCreatedEvent { }").unwrap();

        // Create versioned folder with old version
        fs::create_dir_all(root.join("_versioned")).unwrap();
        fs::write(
            root.join("_versioned/TaskCreatedEvent.v1.ts"),
            "export class TaskCreatedEvent { }",
        )
        .unwrap();

        let config = create_test_config();
        let scanner = EventScanner::new(&config, root);

        let events = scanner.scan().unwrap();
        assert_eq!(events.len(), 2); // Current + old version
    }

    #[test]
    fn test_extract_event_info_simple() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let config = create_test_config();
        let scanner = EventScanner::new(&config, root);

        let (name, event_type, version) =
            scanner.extract_event_info("TaskCreatedEvent.ts", false).unwrap();
        assert_eq!(name, "TaskCreatedEvent");
        assert_eq!(event_type, "TaskCreated");
        assert_eq!(version, EventVersion::Simple("v1".to_string()));
    }

    #[test]
    fn test_extract_event_info_with_version() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let config = create_test_config();
        let scanner = EventScanner::new(&config, root);

        let (name, event_type, version) =
            scanner.extract_event_info("TaskCreatedEvent.v2.ts", false).unwrap();
        assert_eq!(name, "TaskCreatedEvent");
        assert_eq!(event_type, "TaskCreated");
        assert_eq!(version, EventVersion::Simple("v2".to_string()));
    }

    #[test]
    fn test_extract_event_info_semver() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let config = create_test_config();
        let scanner = EventScanner::new(&config, root);

        let (name, event_type, version) =
            scanner.extract_event_info("TaskCreatedEvent.2.1.0.ts", false).unwrap();
        assert_eq!(name, "TaskCreatedEvent");
        assert_eq!(event_type, "TaskCreated");
        assert_eq!(version, EventVersion::Semver(2, 1, 0));
    }

    #[test]
    fn test_parse_event() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();
        let file_path = root.join("TaskCreatedEvent.ts");

        fs::write(&file_path, "export class TaskCreatedEvent { }").unwrap();

        let config = create_test_config();
        let scanner = EventScanner::new(&config, root);

        let event = scanner.parse_event(&file_path, "TaskCreatedEvent.ts", false).unwrap().unwrap();

        assert_eq!(event.name, "TaskCreatedEvent");
        assert_eq!(event.event_type, "TaskCreated");
        assert_eq!(event.version, EventVersion::Simple("v1".to_string()));
    }

    #[test]
    fn test_scan_skips_upcasters_folder() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create events
        fs::write(root.join("TaskCreatedEvent.ts"), "export class TaskCreatedEvent { }").unwrap();

        // Create _upcasters folder with files
        fs::create_dir_all(root.join("_upcasters")).unwrap();
        fs::write(
            root.join("_upcasters/TaskCreated_v1_to_v2.ts"),
            "export class TaskCreated_v1_to_v2 { }",
        )
        .unwrap();

        let config = create_test_config();
        let scanner = EventScanner::new(&config, root);

        let events = scanner.scan().unwrap();

        // Should only find the event, not the upcaster
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].name, "TaskCreatedEvent");
    }
}
