//! Event metadata

use std::path::PathBuf;

/// Metadata for a domain event
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    /// Name of the event class (e.g., "TaskCreatedEvent")
    pub name: String,

    /// Event type identifier (e.g., "TaskCreated")
    pub event_type: String,

    /// Event version
    pub version: EventVersion,

    /// File path relative to project root
    pub file_path: PathBuf,

    /// Event fields/properties
    pub fields: Vec<EventField>,

    /// Whether the @Event decorator is present
    pub decorator_present: bool,
}

impl Event {
    /// Check if this event has a specific field
    pub fn has_field(&self, field_name: &str) -> bool {
        self.fields.iter().any(|f| f.name == field_name)
    }

    /// Check if this is a versioned event (not v1)
    pub fn is_versioned(&self) -> bool {
        match &self.version {
            EventVersion::Simple(v) => v != "v1",
            EventVersion::Semver(major, _, _) => *major > 1,
        }
    }

    /// Check if this is the latest version (in _versioned folder = old version)
    pub fn is_latest(&self) -> bool {
        !self.file_path.to_string_lossy().contains("_versioned")
    }

    /// Get version as string
    pub fn version_string(&self) -> String {
        self.version.to_string()
    }
}

/// Event version representation
#[derive(Debug, Clone, PartialEq)]
pub enum EventVersion {
    /// Simple version format (e.g., "v1", "v2")
    Simple(String),

    /// Semantic version format (e.g., 1.0.0)
    Semver(u32, u32, u32),
}

impl EventVersion {
    /// Parse a version string
    pub fn parse(version_str: &str) -> Option<Self> {
        // Try simple format first
        if version_str.starts_with('v') {
            return Some(EventVersion::Simple(version_str.to_string()));
        }

        // Try semver format
        let parts: Vec<&str> = version_str.split('.').collect();
        if parts.len() == 3 {
            if let (Ok(major), Ok(minor), Ok(patch)) =
                (parts[0].parse::<u32>(), parts[1].parse::<u32>(), parts[2].parse::<u32>())
            {
                return Some(EventVersion::Semver(major, minor, patch));
            }
        }

        None
    }

    /// Convert to string representation
    pub fn as_str(&self) -> &str {
        match self {
            EventVersion::Simple(s) => s.as_str(),
            EventVersion::Semver(_, _, _) => {
                // For now, return a static string since we can't return a temporary
                // In real usage, version_string() should be used for owned String
                "semver"
            }
        }
    }
}

impl std::fmt::Display for EventVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EventVersion::Simple(s) => write!(f, "{s}"),
            EventVersion::Semver(major, minor, patch) => write!(f, "{major}.{minor}.{patch}"),
        }
    }
}

/// Metadata for an event field
#[derive(Debug, Clone, PartialEq)]
pub struct EventField {
    /// Field name
    pub name: String,

    /// Field type (e.g., "string", "number", "Date")
    pub field_type: String,

    /// Whether the field is required
    pub required: bool,

    /// Line number in the file
    pub line_number: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_event_v1() -> Event {
        Event {
            name: "TaskCreatedEvent".to_string(),
            event_type: "TaskCreated".to_string(),
            version: EventVersion::Simple("v1".to_string()),
            file_path: PathBuf::from("domain/events/TaskCreatedEvent.ts"),
            fields: vec![
                EventField {
                    name: "aggregateId".to_string(),
                    field_type: "string".to_string(),
                    required: true,
                    line_number: 5,
                },
                EventField {
                    name: "title".to_string(),
                    field_type: "string".to_string(),
                    required: true,
                    line_number: 6,
                },
            ],
            decorator_present: true,
        }
    }

    fn create_test_event_v2() -> Event {
        Event {
            name: "TaskCreatedEvent".to_string(),
            event_type: "TaskCreated".to_string(),
            version: EventVersion::Simple("v2".to_string()),
            file_path: PathBuf::from("domain/events/_versioned/TaskCreatedEvent.v2.ts"),
            fields: vec![
                EventField {
                    name: "aggregateId".to_string(),
                    field_type: "string".to_string(),
                    required: true,
                    line_number: 5,
                },
                EventField {
                    name: "title".to_string(),
                    field_type: "string".to_string(),
                    required: true,
                    line_number: 6,
                },
                EventField {
                    name: "createdBy".to_string(),
                    field_type: "string".to_string(),
                    required: true,
                    line_number: 7,
                },
            ],
            decorator_present: true,
        }
    }

    #[test]
    fn test_has_field() {
        let event = create_test_event_v1();

        assert!(event.has_field("aggregateId"));
        assert!(event.has_field("title"));
        assert!(!event.has_field("nonExistent"));
    }

    #[test]
    fn test_is_versioned() {
        let v1 = create_test_event_v1();
        assert!(!v1.is_versioned()); // v1 is not considered "versioned"

        let v2 = create_test_event_v2();
        assert!(v2.is_versioned()); // v2+ are versioned
    }

    #[test]
    fn test_is_latest() {
        let v1 = create_test_event_v1();
        assert!(v1.is_latest()); // Not in _versioned folder

        let v2 = create_test_event_v2();
        assert!(!v2.is_latest()); // In _versioned folder
    }

    #[test]
    fn test_version_string() {
        let v1 = create_test_event_v1();
        assert_eq!(v1.version_string(), "v1");

        let v2 = create_test_event_v2();
        assert_eq!(v2.version_string(), "v2");

        let semver_event = Event {
            name: "TaskCreatedEvent".to_string(),
            event_type: "TaskCreated".to_string(),
            version: EventVersion::Semver(2, 1, 0),
            file_path: PathBuf::from("domain/events/TaskCreatedEvent.ts"),
            fields: vec![],
            decorator_present: true,
        };
        assert_eq!(semver_event.version_string(), "2.1.0");
    }

    #[test]
    fn test_event_version_parse() {
        // Simple version
        let v1 = EventVersion::parse("v1").unwrap();
        assert_eq!(v1, EventVersion::Simple("v1".to_string()));

        let v2 = EventVersion::parse("v2").unwrap();
        assert_eq!(v2, EventVersion::Simple("v2".to_string()));

        // Semver
        let semver = EventVersion::parse("2.1.0").unwrap();
        assert_eq!(semver, EventVersion::Semver(2, 1, 0));

        // Invalid
        assert!(EventVersion::parse("invalid").is_none());
        assert!(EventVersion::parse("1.2").is_none());
    }

    #[test]
    fn test_decorator_present() {
        let event = create_test_event_v1();
        assert!(event.decorator_present);

        let event_without_decorator = Event {
            name: "TaskCreatedEvent".to_string(),
            event_type: "TaskCreated".to_string(),
            version: EventVersion::Simple("v1".to_string()),
            file_path: PathBuf::from("domain/events/TaskCreatedEvent.ts"),
            fields: vec![],
            decorator_present: false,
        };
        assert!(!event_without_decorator.decorator_present);
    }
}
