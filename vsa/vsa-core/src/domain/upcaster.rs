//! Upcaster metadata

use std::path::PathBuf;

/// Metadata for an event upcaster
#[derive(Debug, Clone, PartialEq)]
pub struct Upcaster {
    /// Event type this upcaster transforms (e.g., "TaskCreated")
    pub event_type: String,

    /// Source version (e.g., "v1")
    pub from_version: String,

    /// Target version (e.g., "v2")
    pub to_version: String,

    /// File path relative to project root
    pub file_path: PathBuf,

    /// Whether the @Upcaster decorator is present
    pub decorator_present: bool,
}

impl Upcaster {
    /// Get the upcaster name based on convention
    /// e.g., "TaskCreated_v1_to_v2"
    pub fn conventional_name(&self) -> String {
        format!("{}_{}_{}_{}", self.event_type, self.from_version, "to", self.to_version)
    }

    /// Check if this upcaster transforms from a specific version
    pub fn transforms_from(&self, version: &str) -> bool {
        self.from_version == version
    }

    /// Check if this upcaster transforms to a specific version
    pub fn transforms_to(&self, version: &str) -> bool {
        self.to_version == version
    }

    /// Check if this is an incremental upcaster (e.g., v1 -> v2, not v1 -> v3)
    pub fn is_incremental(&self) -> bool {
        // Simple version check: v1 -> v2, v2 -> v3, etc.
        if self.from_version.starts_with('v') && self.to_version.starts_with('v') {
            if let (Ok(from), Ok(to)) = (
                self.from_version.trim_start_matches('v').parse::<u32>(),
                self.to_version.trim_start_matches('v').parse::<u32>(),
            ) {
                return to == from + 1;
            }
        }

        // For semver, we'd need more complex logic
        // For now, assume non-simple versions are incremental
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_upcaster_v1_to_v2() -> Upcaster {
        Upcaster {
            event_type: "TaskCreated".to_string(),
            from_version: "v1".to_string(),
            to_version: "v2".to_string(),
            file_path: PathBuf::from("domain/events/_upcasters/TaskCreated_v1_to_v2.ts"),
            decorator_present: true,
        }
    }

    fn create_test_upcaster_v2_to_v3() -> Upcaster {
        Upcaster {
            event_type: "TaskCreated".to_string(),
            from_version: "v2".to_string(),
            to_version: "v3".to_string(),
            file_path: PathBuf::from("domain/events/_upcasters/TaskCreated_v2_to_v3.ts"),
            decorator_present: true,
        }
    }

    fn create_test_upcaster_non_incremental() -> Upcaster {
        Upcaster {
            event_type: "TaskCreated".to_string(),
            from_version: "v1".to_string(),
            to_version: "v3".to_string(),
            file_path: PathBuf::from("domain/events/_upcasters/TaskCreated_v1_to_v3.ts"),
            decorator_present: true,
        }
    }

    #[test]
    fn test_conventional_name() {
        let upcaster = create_test_upcaster_v1_to_v2();
        assert_eq!(upcaster.conventional_name(), "TaskCreated_v1_to_v2");

        let upcaster_v2_to_v3 = create_test_upcaster_v2_to_v3();
        assert_eq!(upcaster_v2_to_v3.conventional_name(), "TaskCreated_v2_to_v3");
    }

    #[test]
    fn test_transforms_from() {
        let upcaster = create_test_upcaster_v1_to_v2();

        assert!(upcaster.transforms_from("v1"));
        assert!(!upcaster.transforms_from("v2"));
        assert!(!upcaster.transforms_from("v3"));
    }

    #[test]
    fn test_transforms_to() {
        let upcaster = create_test_upcaster_v1_to_v2();

        assert!(!upcaster.transforms_to("v1"));
        assert!(upcaster.transforms_to("v2"));
        assert!(!upcaster.transforms_to("v3"));
    }

    #[test]
    fn test_is_incremental() {
        // v1 -> v2 is incremental
        let incremental = create_test_upcaster_v1_to_v2();
        assert!(incremental.is_incremental());

        // v2 -> v3 is incremental
        let incremental_v2_v3 = create_test_upcaster_v2_to_v3();
        assert!(incremental_v2_v3.is_incremental());

        // v1 -> v3 is NOT incremental
        let non_incremental = create_test_upcaster_non_incremental();
        assert!(!non_incremental.is_incremental());
    }

    #[test]
    fn test_decorator_present() {
        let upcaster = create_test_upcaster_v1_to_v2();
        assert!(upcaster.decorator_present);

        let upcaster_without_decorator = Upcaster {
            event_type: "TaskCreated".to_string(),
            from_version: "v1".to_string(),
            to_version: "v2".to_string(),
            file_path: PathBuf::from("domain/events/_upcasters/TaskCreated_v1_to_v2.ts"),
            decorator_present: false,
        };
        assert!(!upcaster_without_decorator.decorator_present);
    }
}
