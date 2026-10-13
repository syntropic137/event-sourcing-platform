//! Slice scanner and type detection
//!
//! Scans for vertical slices and detects their type (command, query, saga).
//! Slices are found in `<root>/<slices.path>/` (one context at the root) and in
//! `<root>/<context>/<slices.path>/` for every context directory under the root
//! (the hexagonal, multi-context layout). A slice's files are read recursively,
//! so `internal/` and other subfolders count.
//!
//! Slice type can be determined:
//! 1. Explicitly via `slice.yaml` metadata file (`type:`, or section keys)
//! 2. Implicitly from file names (has *Command.* = command, has *Query.* = query, etc.)

use crate::config::{ArchitectureType, SliceType, SlicesConfig, VsaConfig};
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
    #[serde(default)]
    pub name: String,

    /// Explicit slice type as written (`type:`), empty when absent
    #[serde(rename = "type", default)]
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

    /// Type implied by section keys (`command:`, `query:`, `projection:`,
    /// `read_model:`) when there is no `type:`
    #[serde(skip)]
    pub inferred_type: Option<SliceType>,
}

impl SliceManifest {
    /// Build a manifest from parsed YAML without failing on optional or
    /// differently-shaped keys.
    pub fn from_yaml(value: &serde_yaml::Value) -> Self {
        let get = |key: &str| value.get(key);
        let string = |key: &str| get(key).and_then(|v| v.as_str()).map(str::to_string);
        // `projection:` is either a name or a mapping with a `name:`.
        let projection = get("projection").and_then(|v| {
            v.as_str().map(str::to_string).or_else(|| v.get("name")?.as_str().map(str::to_string))
        });
        let subscribes_to = get("subscribes_to")
            .and_then(|v| v.as_sequence())
            .map(|seq| seq.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
            .unwrap_or_default();

        let has = |key: &str| get(key).is_some();
        let has_command = has("command");
        let has_view = has("query") || has("projection") || has("read_model");
        let inferred_type = match (has_command, has_view) {
            (true, true) => Some(SliceType::Mixed),
            (false, true) => Some(SliceType::Query),
            (true, false) => Some(SliceType::Command),
            (false, false) => None,
        };

        Self {
            name: string("name").unwrap_or_default(),
            slice_type: string("type").unwrap_or_default(),
            projection,
            subscribes_to,
            returns: string("returns"),
            description: string("description"),
            inferred_type,
        }
    }
}

/// Scanner for finding and analyzing slices
pub struct SliceScanner<'a> {
    config: Option<&'a SlicesConfig>,
    root: &'a Path,
    per_context: bool,
}

impl<'a> SliceScanner<'a> {
    /// Create a new slice scanner
    pub fn new(config: Option<&'a SlicesConfig>, root: &'a Path) -> Self {
        Self { config, root, per_context: false }
    }

    /// Scanner for validation rules: also scans `<root>/<context>/<slices>/`
    /// when the architecture keeps slices per bounded context
    /// (`hexagonal-event-sourced-vsa`). Other architectures keep the
    /// single-root scan until their consumers have been checked against the
    /// wider one.
    pub fn for_config(config: &'a VsaConfig, root: &'a Path) -> Self {
        let per_context = config.architecture == ArchitectureType::HexagonalEventSourcedVsa;
        Self { config: config.slices.as_ref(), root, per_context }
    }

    /// Also scan `<root>/<context>/<slices>/` for every context under the root.
    pub fn per_context(mut self, enabled: bool) -> Self {
        self.per_context = enabled;
        self
    }

    /// Scan for all slices
    pub fn scan(&self) -> Result<Vec<Slice>> {
        let mut slices = Vec::new();
        for dir in self.slice_directories()? {
            self.scan_slices_directory(&dir, &mut slices)?;
        }
        Ok(slices)
    }

    /// Directories that hold slices.
    ///
    /// `<root>/<path>` covers a project whose root is a single context.
    /// `<root>/<context>/<path>` covers the multi-context layout, where each
    /// bounded context keeps its own `slices/`. Before both were scanned, a
    /// multi-context project had no slices as far as the slice rules could see.
    fn slice_directories(&self) -> Result<Vec<PathBuf>> {
        let relative =
            self.config.map(|c| c.path.clone()).unwrap_or_else(|| PathBuf::from("slices"));
        let mut dirs = Vec::new();

        let direct = self.root.join(&relative);
        if direct.is_dir() {
            dirs.push(direct);
        }

        if !self.per_context || !self.root.is_dir() {
            return Ok(dirs);
        }

        for context in sorted_subdirectories(self.root)? {
            let candidate = context.join(&relative);
            if candidate.is_dir() && !dirs.contains(&candidate) {
                dirs.push(candidate);
            }
        }

        Ok(dirs)
    }

    /// Scan a slices directory for individual slice folders
    fn scan_slices_directory(&self, dir: &Path, slices: &mut Vec<Slice>) -> Result<()> {
        for path in sorted_subdirectories(dir)? {
            if let Some(slice) = self.analyze_slice(&path)? {
                slices.push(slice);
            }
        }

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

    /// Scan files within a slice directory, recursively.
    ///
    /// Subfolders (`internal/`, `handlers/`) are part of the slice. Hidden
    /// folders and build/cache folders are not.
    fn scan_slice_files(&self, slice_path: &Path) -> Result<Vec<SliceFile>> {
        let mut files = Vec::new();

        if !slice_path.exists() || !slice_path.is_dir() {
            return Ok(files);
        }

        let walker = walkdir::WalkDir::new(slice_path)
            .sort_by_file_name()
            .into_iter()
            .filter_entry(|e| e.depth() == 0 || !is_skipped_dir(e));
        for entry in walker {
            let entry = entry.map_err(std::io::Error::from)?;
            if !entry.file_type().is_file() {
                continue;
            }
            if let Some(name) = entry.file_name().to_str() {
                let file_type = self.classify_file(name);
                files.push(SliceFile {
                    name: name.to_string(),
                    path: entry.path().to_path_buf(),
                    file_type,
                });
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
            || name.contains(".spec.")
            || name.contains("_test.")
            || name.starts_with("test_")
            || name == "conftest.py"
        {
            return SliceFileType::Test;
        }

        // Check for slice manifest
        if name == "slice.yaml" || name == "slice.yml" {
            return SliceFileType::Manifest;
        }

        // Classify by role: PascalCase suffix (`PlaceOrderHandler.ts`),
        // snake_case suffix (`place_order_handler.rs`) or the bare role name
        // (`handler.py`, `projection.py`).
        let has_role = |pascal: &str, snake: &str| {
            base.ends_with(pascal) || base.ends_with(&format!("_{snake}")) || base == snake
        };
        if has_role("Command", "command") {
            SliceFileType::Command
        } else if has_role("Query", "query") {
            SliceFileType::Query
        } else if has_role("Event", "event") {
            SliceFileType::Event
        } else if has_role("Handler", "handler") {
            SliceFileType::Handler
        } else if has_role("Projection", "projection") {
            SliceFileType::Projection
        } else if has_role("Controller", "controller") || has_role("Adapter", "adapter") {
            SliceFileType::Controller
        } else if has_role("Saga", "saga") {
            SliceFileType::Saga
        } else {
            SliceFileType::Other
        }
    }

    /// Read and parse slice.yaml if present.
    ///
    /// The manifest is read leniently: `type:` may be absent (the type is then
    /// inferred from section keys such as `query:` or `projection:`), and
    /// `projection:` may be a name or a mapping with a `name:`. A file that is
    /// not valid YAML is reported on stderr and treated as absent, so one bad
    /// manifest does not stop every slice rule.
    fn read_slice_manifest(&self, slice_path: &Path) -> Result<Option<SliceManifest>> {
        let yaml_path = slice_path.join("slice.yaml");
        let yml_path = slice_path.join("slice.yml");

        let path = if yaml_path.exists() {
            yaml_path
        } else if yml_path.exists() {
            yml_path
        } else {
            return Ok(None);
        };

        let content = fs::read_to_string(&path)?;
        match serde_yaml::from_str::<serde_yaml::Value>(&content) {
            Ok(value) => Ok(Some(SliceManifest::from_yaml(&value))),
            Err(e) => {
                eprintln!("  ⚠ Warning: could not parse {}: {}", path.display(), e);
                Ok(None)
            }
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
            if !manifest.slice_type.is_empty() {
                // Unknown for unrecognized types; VSA036 reports them.
                let slice_type =
                    SliceType::from_name(&manifest.slice_type).unwrap_or(SliceType::Unknown);
                return (slice_type, true);
            }
            // No `type:`: section keys in slice.yaml say what the slice is.
            if let Some(slice_type) = manifest.inferred_type.clone() {
                return (slice_type, false);
            }
        }

        // 2. Implicit: Detect from file contents
        let has_command = files.iter().any(|f| f.file_type == SliceFileType::Command);
        let has_query = files.iter().any(|f| f.file_type == SliceFileType::Query);
        let has_projection = files.iter().any(|f| f.file_type == SliceFileType::Projection);
        let has_saga = files.iter().any(|f| f.file_type == SliceFileType::Saga);

        // Priority: Saga > Mixed > Query > Command > Unknown
        let slice_type = if has_saga {
            SliceType::Saga
        } else if has_command && (has_query || has_projection) {
            SliceType::Mixed
        } else if has_query || has_projection {
            SliceType::Query
        } else if has_command {
            SliceType::Command
        } else {
            // Unknown if we can't determine
            SliceType::Unknown
        };

        (slice_type, false)
    }
}

/// Subdirectories of `dir`, sorted, skipping hidden, `_`-prefixed and
/// build/cache folders. A directory that cannot be read is an error, so a scan
/// never reports success over slices it could not see.
fn sorted_subdirectories(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut dirs = Vec::new();
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        let keep = path.is_dir()
            && path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| !n.starts_with('.') && !n.starts_with('_') && !is_skipped_name(n));
        if keep {
            dirs.push(path);
        }
    }
    dirs.sort();
    Ok(dirs)
}

fn is_skipped_name(name: &str) -> bool {
    matches!(name, "node_modules" | "__pycache__" | "target" | "dist" | "build")
}

fn is_skipped_dir(entry: &walkdir::DirEntry) -> bool {
    entry.file_type().is_dir()
        && entry.file_name().to_str().is_some_and(|n| n.starts_with('.') || is_skipped_name(n))
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
            inferred_type: None,
        });

        let (slice_type, explicit) = scanner.detect_slice_type(&files, &manifest);
        assert_eq!(slice_type, SliceType::Saga);
        assert!(explicit);
    }

    #[test]
    fn test_detect_slice_type_mixed() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let config = create_test_config();
        let scanner = SliceScanner::new(Some(&config), root);

        // Slice with both command and query/projection files
        let files = vec![
            SliceFile {
                name: "CreateOrderCommand.ts".to_string(),
                path: PathBuf::from("CreateOrderCommand.ts"),
                file_type: SliceFileType::Command,
            },
            SliceFile {
                name: "GetOrderQuery.ts".to_string(),
                path: PathBuf::from("GetOrderQuery.ts"),
                file_type: SliceFileType::Query,
            },
        ];

        let (slice_type, explicit) = scanner.detect_slice_type(&files, &None);
        assert_eq!(slice_type, SliceType::Mixed);
        assert!(!explicit);
    }

    #[test]
    fn test_detect_slice_type_unknown() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let config = create_test_config();
        let scanner = SliceScanner::new(Some(&config), root);

        // Slice with no recognized file types
        let files = vec![
            SliceFile {
                name: "utils.ts".to_string(),
                path: PathBuf::from("utils.ts"),
                file_type: SliceFileType::Other,
            },
            SliceFile {
                name: "helpers.ts".to_string(),
                path: PathBuf::from("helpers.ts"),
                file_type: SliceFileType::Other,
            },
        ];

        let (slice_type, explicit) = scanner.detect_slice_type(&files, &None);
        assert_eq!(slice_type, SliceType::Unknown);
        assert!(!explicit);
    }

    #[test]
    fn test_detect_slice_type_explicit_mixed() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let config = create_test_config();
        let scanner = SliceScanner::new(Some(&config), root);

        let files = vec![];
        let manifest = Some(SliceManifest {
            name: "test_slice".to_string(),
            slice_type: "mixed".to_string(),
            projection: None,
            subscribes_to: vec![],
            returns: None,
            description: None,
            inferred_type: None,
        });

        let (slice_type, explicit) = scanner.detect_slice_type(&files, &manifest);
        assert_eq!(slice_type, SliceType::Mixed);
        assert!(explicit);
    }

    #[test]
    fn test_detect_slice_type_explicit_unknown_string() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let config = create_test_config();
        let scanner = SliceScanner::new(Some(&config), root);

        let files = vec![];
        let manifest = Some(SliceManifest {
            name: "test_slice".to_string(),
            slice_type: "invalid_type".to_string(), // Unrecognized type
            projection: None,
            subscribes_to: vec![],
            returns: None,
            description: None,
            inferred_type: None,
        });

        let (slice_type, explicit) = scanner.detect_slice_type(&files, &manifest);
        assert_eq!(slice_type, SliceType::Unknown);
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

    #[test]
    fn finds_slices_in_every_context_of_a_multi_context_root() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();
        // Hexagonal layout: root/<context>/slices/<slice>/, nested handler, snake_case names.
        let place = root.join("orders/slices/place_order/internal");
        fs::create_dir_all(&place).unwrap();
        fs::write(
            root.join("orders/slices/place_order/slice.yaml"),
            "name: place_order\ntype: command\n",
        )
        .unwrap();
        fs::write(place.join("place_order_handler.rs"), "pub fn place() {}").unwrap();
        fs::write(place.join("place_order_handler_test.rs"), "#[test] fn t() {}").unwrap();
        let list = root.join("billing/slices/list_invoices");
        fs::create_dir_all(&list).unwrap();
        fs::write(list.join("list_invoices_projection.rs"), "").unwrap();
        // Not slices: hidden and private context folders.
        fs::create_dir_all(root.join("_shared/slices/not_a_slice")).unwrap();
        fs::create_dir_all(root.join(".cache/slices/not_a_slice")).unwrap();

        let config = create_test_config();
        let single_root = SliceScanner::new(Some(&config), root).scan().unwrap();
        assert!(single_root.is_empty(), "per-context scanning is opt-in");
        let slices = SliceScanner::new(Some(&config), root).per_context(true).scan().unwrap();

        let names: Vec<&str> = slices.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["list_invoices", "place_order"]);
        let place = &slices[1];
        assert_eq!(place.slice_type, SliceType::Command);
        assert!(place.has_handler(), "nested snake_case handler is found");
        assert!(place.has_tests());
        assert!(slices[0].has_projection());
        assert_eq!(slices[0].slice_type, SliceType::Query);
    }

    #[test]
    fn classifies_snake_case_and_bare_role_names() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config();
        let scanner = SliceScanner::new(Some(&config), temp_dir.path());
        assert_eq!(scanner.classify_file("place_order_handler.rs"), SliceFileType::Handler);
        assert_eq!(scanner.classify_file("handler.py"), SliceFileType::Handler);
        assert_eq!(scanner.classify_file("projection.py"), SliceFileType::Projection);
        assert_eq!(scanner.classify_file("order_list_projection.rs"), SliceFileType::Projection);
        assert_eq!(scanner.classify_file("place_order_command.rs"), SliceFileType::Command);
        assert_eq!(scanner.classify_file("test_projection.py"), SliceFileType::Test);
        assert_eq!(scanner.classify_file("place_handler_test.rs"), SliceFileType::Test);
        assert_eq!(scanner.classify_file("conftest.py"), SliceFileType::Test);
        // A role word inside a name is not a role.
        assert_eq!(scanner.classify_file("handlers_registry.py"), SliceFileType::Other);
        assert_eq!(scanner.classify_file("mod.rs"), SliceFileType::Other);
    }

    #[test]
    fn manifest_without_type_and_with_projection_mapping_is_read() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();
        let slice = root.join("slices/get_installation");
        fs::create_dir_all(&slice).unwrap();
        fs::write(
            slice.join("slice.yaml"),
            "name: get_installation\nquery:\n  name: GetInstallationQuery\nprojection:\n  name: InstallationProjection\n  events: [github.AppInstalled]\n",
        )
        .unwrap();

        let config = create_test_config();
        let slices = SliceScanner::new(Some(&config), root).scan().unwrap();

        assert_eq!(slices.len(), 1);
        assert_eq!(slices[0].slice_type, SliceType::Query);
        assert!(!slices[0].type_explicit);
        let manifest = slices[0].metadata.as_ref().unwrap();
        assert_eq!(manifest.projection.as_deref(), Some("InstallationProjection"));
    }

    #[test]
    fn unparsable_manifest_does_not_abort_the_scan() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();
        let slice = root.join("slices/broken");
        fs::create_dir_all(&slice).unwrap();
        fs::write(slice.join("slice.yaml"), "name: [unclosed\n").unwrap();

        let config = create_test_config();
        let slices = SliceScanner::new(Some(&config), root).scan().unwrap();
        assert_eq!(slices.len(), 1);
        assert!(slices[0].metadata.is_none());
    }
}
