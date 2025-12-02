//! Slice scanner and type detection
//!
//! Scans for vertical slices and detects their type (command, query, saga).
//! Slice type can be determined:
//! 1. Explicitly via `slice.yaml` metadata file
//! 2. Implicitly from file contents (has *Command.* = command, has *Query.* = query, etc.)

use crate::config::{SliceType, SlicesConfig};
use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Metadata for a slice (vertical feature slice)
#[derive(Debug, Clone, PartialEq)]
pub struct Slice {
    /// Name of the slice (e.g., "list_workflows", "create_order")
    pub name: String,

    /// File path to the slice directory
    pub path: PathBuf,

    /// Detected or explicit slice type
    pub slice_type: SliceType,

    /// Whether type was explicitly defined in slice.yaml
    pub type_explicit: bool,

    /// Files in this slice
    pub files: Vec<SliceFile>,

    /// Parsed slice.yaml metadata (if present)
    pub metadata: Option<SliceManifest>,
}

/// A file within a slice
#[derive(Debug, Clone, PartialEq)]
pub struct SliceFile {
    /// File name
    pub name: String,

    /// Full path
    pub path: PathBuf,

    /// File type (inferred from name)
    pub file_type: SliceFileType,
}

/// Type of file within a slice
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SliceFileType {
    /// Command definition (*Command.*)
    Command,
    /// Query definition (*Query.*)
    Query,
    /// Event definition (*Event.*)
    Event,
    /// Handler (*Handler.*)
    Handler,
    /// Projection (*Projection.*)
    Projection,
    /// Controller/Adapter (*Controller.*, *Adapter.*)
    Controller,
    /// Test file (*.test.*, *_test.*, test_*.*)
    Test,
    /// Saga (*Saga.*)
    Saga,
    /// Slice manifest (slice.yaml)
    Manifest,
    /// Other file
    Other,
}

/// Slice manifest (slice.yaml)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SliceManifest {
    /// Slice name
    pub name: String,

    /// Explicit slice type
    #[serde(rename = "type")]
    pub slice_type: String,

    /// For query slices: the projection class name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub projection: Option<String>,

    /// Events this slice subscribes to
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subscribes_to: Vec<String>,

    /// Return type (for query slices)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub returns: Option<String>,

    /// Description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Scanner for finding and analyzing slices
pub struct SliceScanner<'a> {
    #[allow(dead_code)]
    config: Option<&'a SlicesConfig>,
    root: &'a Path,
}

impl<'a> SliceScanner<'a> {
    /// Create a new slice scanner
    pub fn new(config: Option<&'a SlicesConfig>, root: &'a Path) -> Self {
        Self { config, root }
    }

    /// Scan for all slices
    pub fn scan(&self) -> Result<Vec<Slice>> {
        let mut slices = Vec::new();

        // If we have a slices config, use the configured path
        let slices_path = if let Some(config) = self.config {
            self.root.join(&config.path)
        } else {
            // Default: look in "slices" directory
            self.root.join("slices")
        };

        if slices_path.exists() && slices_path.is_dir() {
            self.scan_slices_directory(&slices_path, &mut slices)?;
        }

        // Also scan for legacy structure (contexts with features)
        // This maintains backward compatibility with v1 VSA config
        self.scan_legacy_structure(self.root, &mut slices)?;

        Ok(slices)
    }

    /// Scan a slices directory for individual slice folders
    fn scan_slices_directory(&self, dir: &Path, slices: &mut Vec<Slice>) -> Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                // Skip hidden directories
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if !name.starts_with('.') && !name.starts_with('_') {
                        if let Some(slice) = self.analyze_slice(&path)? {
                            slices.push(slice);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Scan legacy structure (contexts/features)
    #[allow(clippy::ptr_arg)]
    fn scan_legacy_structure(&self, _dir: &Path, _slices: &mut Vec<Slice>) -> Result<()> {
        // TODO: Implement legacy structure scanning if needed
        // For now, we focus on the modern slices/ structure
        Ok(())
    }

    /// Analyze a single slice directory
    fn analyze_slice(&self, slice_path: &Path) -> Result<Option<Slice>> {
        let name = slice_path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
            .unwrap_or_default();

        if name.is_empty() {
            return Ok(None);
        }

        // Scan files in the slice
        let files = self.scan_slice_files(slice_path)?;

        // Try to read slice.yaml
        let metadata = self.read_slice_manifest(slice_path)?;

        // Detect slice type
        let (slice_type, type_explicit) = self.detect_slice_type(&files, &metadata);

        Ok(Some(Slice {
            name,
            path: slice_path.to_path_buf(),
            slice_type,
            type_explicit,
            files,
            metadata,
        }))
    }

    /// Scan files within a slice directory
    fn scan_slice_files(&self, slice_path: &Path) -> Result<Vec<SliceFile>> {
        let mut files = Vec::new();

        if !slice_path.exists() || !slice_path.is_dir() {
            return Ok(files);
        }

        for entry in fs::read_dir(slice_path)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    let file_type = self.classify_file(name);
                    files.push(SliceFile { name: name.to_string(), path: path.clone(), file_type });
                }
            }
        }

        Ok(files)
    }

    /// Classify a file based on its name
    fn classify_file(&self, name: &str) -> SliceFileType {
        // Remove extension for classification
        let base = name
            .strip_suffix(".ts")
            .or_else(|| name.strip_suffix(".tsx"))
            .or_else(|| name.strip_suffix(".py"))
            .or_else(|| name.strip_suffix(".rs"))
            .unwrap_or(name);

        // Check for test files first (they might contain other keywords)
        if name.contains(".test.")
            || name.contains("_test.")
            || name.starts_with("test_")
            || name.ends_with("_test.py")
            || name.ends_with("_test.ts")
        {
            return SliceFileType::Test;
        }

        // Check for slice manifest
        if name == "slice.yaml" || name == "slice.yml" {
            return SliceFileType::Manifest;
        }

        // Classify by suffix
        if base.ends_with("Command") {
            SliceFileType::Command
        } else if base.ends_with("Query") {
            SliceFileType::Query
        } else if base.ends_with("Event") {
            SliceFileType::Event
        } else if base.ends_with("Handler") {
            SliceFileType::Handler
        } else if base.ends_with("Projection") {
            SliceFileType::Projection
        } else if base.ends_with("Controller") || base.ends_with("Adapter") {
            SliceFileType::Controller
        } else if base.ends_with("Saga") {
            SliceFileType::Saga
        } else {
            SliceFileType::Other
        }
    }

    /// Read and parse slice.yaml if present
    fn read_slice_manifest(&self, slice_path: &Path) -> Result<Option<SliceManifest>> {
        let yaml_path = slice_path.join("slice.yaml");
        let yml_path = slice_path.join("slice.yml");

        let manifest_path = if yaml_path.exists() {
            Some(yaml_path)
        } else if yml_path.exists() {
            Some(yml_path)
        } else {
            None
        };

        if let Some(path) = manifest_path {
            let content = fs::read_to_string(path)?;
            let manifest: SliceManifest = serde_yaml::from_str(&content)?;
            Ok(Some(manifest))
        } else {
            Ok(None)
        }
    }

    /// Detect slice type from files and manifest
    fn detect_slice_type(
        &self,
        files: &[SliceFile],
        metadata: &Option<SliceManifest>,
    ) -> (SliceType, bool) {
        // 1. Explicit: Check slice.yaml first
        if let Some(manifest) = metadata {
            let slice_type = match manifest.slice_type.to_lowercase().as_str() {
                "command" => SliceType::Command,
                "query" => SliceType::Query,
                "saga" => SliceType::Saga,
                _ => SliceType::Command, // Default
            };
            return (slice_type, true);
        }

        // 2. Implicit: Detect from file contents
        let has_command = files.iter().any(|f| f.file_type == SliceFileType::Command);
        let has_query = files.iter().any(|f| f.file_type == SliceFileType::Query);
        let has_projection = files.iter().any(|f| f.file_type == SliceFileType::Projection);
        let has_saga = files.iter().any(|f| f.file_type == SliceFileType::Saga);

        // Priority: Saga > Query (with Projection) > Query > Command
        let slice_type = if has_saga {
            SliceType::Saga
        } else if has_query || has_projection {
            SliceType::Query
        } else if has_command {
            SliceType::Command
        } else {
            // Default to Command if we can't determine
            SliceType::Command
        };

        (slice_type, false)
    }
}

impl Slice {
    /// Check if this slice has a handler
    pub fn has_handler(&self) -> bool {
        self.files.iter().any(|f| f.file_type == SliceFileType::Handler)
    }

    /// Check if this slice has a projection
    pub fn has_projection(&self) -> bool {
        self.files.iter().any(|f| f.file_type == SliceFileType::Projection)
    }

    /// Check if this slice has tests
    pub fn has_tests(&self) -> bool {
        self.files.iter().any(|f| f.file_type == SliceFileType::Test)
    }

    /// Check if this slice has a controller/adapter
    pub fn has_controller(&self) -> bool {
        self.files.iter().any(|f| f.file_type == SliceFileType::Controller)
    }

    /// Get non-test, non-manifest file count (for line count validation)
    pub fn source_file_count(&self) -> usize {
        self.files
            .iter()
            .filter(|f| !matches!(f.file_type, SliceFileType::Test | SliceFileType::Manifest))
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_config() -> SlicesConfig {
        SlicesConfig {
            path: PathBuf::from("slices"),
            types: vec![SliceType::Command, SliceType::Query, SliceType::Saga],
            metadata_file: "slice.yaml".to_string(),
            command: None,
            query: None,
            saga: None,
        }
    }

    #[test]
    fn test_scan_empty_directory() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create empty slices directory
        fs::create_dir_all(root.join("slices")).unwrap();

        let config = create_test_config();
        let scanner = SliceScanner::new(Some(&config), root);

        let slices = scanner.scan().unwrap();
        assert_eq!(slices.len(), 0);
    }

    #[test]
    fn test_scan_command_slice() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create command slice
        let slice_path = root.join("slices/create_order");
        fs::create_dir_all(&slice_path).unwrap();
        fs::write(slice_path.join("CreateOrderCommand.ts"), "export class CreateOrderCommand {}")
            .unwrap();
        fs::write(slice_path.join("CreateOrderHandler.ts"), "export class CreateOrderHandler {}")
            .unwrap();
        fs::write(slice_path.join("OrderCreatedEvent.ts"), "export class OrderCreatedEvent {}")
            .unwrap();
        fs::write(
            slice_path.join("CreateOrderController.ts"),
            "export class CreateOrderController {}",
        )
        .unwrap();

        let config = create_test_config();
        let scanner = SliceScanner::new(Some(&config), root);

        let slices = scanner.scan().unwrap();
        assert_eq!(slices.len(), 1);

        let slice = &slices[0];
        assert_eq!(slice.name, "create_order");
        assert_eq!(slice.slice_type, SliceType::Command);
        assert!(!slice.type_explicit);
        assert!(slice.has_handler());
        assert!(slice.has_controller());
        assert!(!slice.has_projection());
    }

    #[test]
    fn test_scan_query_slice() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create query slice
        let slice_path = root.join("slices/list_orders");
        fs::create_dir_all(&slice_path).unwrap();
        fs::write(slice_path.join("ListOrdersQuery.py"), "class ListOrdersQuery: pass").unwrap();
        fs::write(slice_path.join("OrderListProjection.py"), "class OrderListProjection: pass")
            .unwrap();
        fs::write(slice_path.join("ListOrdersHandler.py"), "class ListOrdersHandler: pass")
            .unwrap();

        let config = create_test_config();
        let scanner = SliceScanner::new(Some(&config), root);

        let slices = scanner.scan().unwrap();
        assert_eq!(slices.len(), 1);

        let slice = &slices[0];
        assert_eq!(slice.name, "list_orders");
        assert_eq!(slice.slice_type, SliceType::Query);
        assert!(!slice.type_explicit);
        assert!(slice.has_handler());
        assert!(slice.has_projection());
    }

    #[test]
    fn test_scan_slice_with_manifest() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create query slice with explicit manifest
        let slice_path = root.join("slices/get_order_details");
        fs::create_dir_all(&slice_path).unwrap();
        fs::write(
            slice_path.join("OrderDetailProjection.ts"),
            "export class OrderDetailProjection {}",
        )
        .unwrap();
        fs::write(
            slice_path.join("GetOrderDetailsHandler.ts"),
            "export class GetOrderDetailsHandler {}",
        )
        .unwrap();

        // Create slice.yaml
        fs::write(
            slice_path.join("slice.yaml"),
            r#"
name: get_order_details
type: query
projection: OrderDetailProjection
subscribes_to:
  - OrderCreatedEvent
  - OrderUpdatedEvent
returns: OrderDetail
"#,
        )
        .unwrap();

        let config = create_test_config();
        let scanner = SliceScanner::new(Some(&config), root);

        let slices = scanner.scan().unwrap();
        assert_eq!(slices.len(), 1);

        let slice = &slices[0];
        assert_eq!(slice.name, "get_order_details");
        assert_eq!(slice.slice_type, SliceType::Query);
        assert!(slice.type_explicit);

        let manifest = slice.metadata.as_ref().unwrap();
        assert_eq!(manifest.projection, Some("OrderDetailProjection".to_string()));
        assert_eq!(manifest.subscribes_to.len(), 2);
        assert_eq!(manifest.returns, Some("OrderDetail".to_string()));
    }

    #[test]
    fn test_classify_file() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let config = create_test_config();
        let scanner = SliceScanner::new(Some(&config), root);

        assert_eq!(scanner.classify_file("CreateOrderCommand.ts"), SliceFileType::Command);
        assert_eq!(scanner.classify_file("ListOrdersQuery.py"), SliceFileType::Query);
        assert_eq!(scanner.classify_file("OrderCreatedEvent.ts"), SliceFileType::Event);
        assert_eq!(scanner.classify_file("CreateOrderHandler.py"), SliceFileType::Handler);
        assert_eq!(scanner.classify_file("OrderListProjection.ts"), SliceFileType::Projection);
        assert_eq!(scanner.classify_file("CreateOrderController.py"), SliceFileType::Controller);
        assert_eq!(scanner.classify_file("OrderProcessingSaga.ts"), SliceFileType::Saga);
        assert_eq!(scanner.classify_file("CreateOrder.test.ts"), SliceFileType::Test);
        assert_eq!(scanner.classify_file("test_create_order.py"), SliceFileType::Test);
        assert_eq!(scanner.classify_file("slice.yaml"), SliceFileType::Manifest);
        assert_eq!(scanner.classify_file("utils.ts"), SliceFileType::Other);
    }

    #[test]
    fn test_detect_slice_type_command() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let config = create_test_config();
        let scanner = SliceScanner::new(Some(&config), root);

        let files = vec![
            SliceFile {
                name: "CreateOrderCommand.ts".to_string(),
                path: PathBuf::from("CreateOrderCommand.ts"),
                file_type: SliceFileType::Command,
            },
            SliceFile {
                name: "CreateOrderHandler.ts".to_string(),
                path: PathBuf::from("CreateOrderHandler.ts"),
                file_type: SliceFileType::Handler,
            },
        ];

        let (slice_type, explicit) = scanner.detect_slice_type(&files, &None);
        assert_eq!(slice_type, SliceType::Command);
        assert!(!explicit);
    }

    #[test]
    fn test_detect_slice_type_query() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let config = create_test_config();
        let scanner = SliceScanner::new(Some(&config), root);

        let files = vec![
            SliceFile {
                name: "ListOrdersQuery.ts".to_string(),
                path: PathBuf::from("ListOrdersQuery.ts"),
                file_type: SliceFileType::Query,
            },
            SliceFile {
                name: "OrderListProjection.ts".to_string(),
                path: PathBuf::from("OrderListProjection.ts"),
                file_type: SliceFileType::Projection,
            },
        ];

        let (slice_type, explicit) = scanner.detect_slice_type(&files, &None);
        assert_eq!(slice_type, SliceType::Query);
        assert!(!explicit);
    }

    #[test]
    fn test_detect_slice_type_explicit() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let config = create_test_config();
        let scanner = SliceScanner::new(Some(&config), root);

        let files = vec![];
        let manifest = Some(SliceManifest {
            name: "test_slice".to_string(),
            slice_type: "saga".to_string(),
            projection: None,
            subscribes_to: vec![],
            returns: None,
            description: None,
        });

        let (slice_type, explicit) = scanner.detect_slice_type(&files, &manifest);
        assert_eq!(slice_type, SliceType::Saga);
        assert!(explicit);
    }

    #[test]
    fn test_slice_has_methods() {
        let slice = Slice {
            name: "test_slice".to_string(),
            path: PathBuf::from("slices/test_slice"),
            slice_type: SliceType::Query,
            type_explicit: false,
            files: vec![
                SliceFile {
                    name: "TestQuery.ts".to_string(),
                    path: PathBuf::from("TestQuery.ts"),
                    file_type: SliceFileType::Query,
                },
                SliceFile {
                    name: "TestProjection.ts".to_string(),
                    path: PathBuf::from("TestProjection.ts"),
                    file_type: SliceFileType::Projection,
                },
                SliceFile {
                    name: "TestHandler.ts".to_string(),
                    path: PathBuf::from("TestHandler.ts"),
                    file_type: SliceFileType::Handler,
                },
                SliceFile {
                    name: "test_slice.test.ts".to_string(),
                    path: PathBuf::from("test_slice.test.ts"),
                    file_type: SliceFileType::Test,
                },
            ],
            metadata: None,
        };

        assert!(slice.has_handler());
        assert!(slice.has_projection());
        assert!(slice.has_tests());
        assert!(!slice.has_controller());
        assert_eq!(slice.source_file_count(), 3); // Excludes test file
    }
}
