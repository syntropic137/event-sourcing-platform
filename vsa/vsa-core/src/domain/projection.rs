//! Projection metadata
//!
//! Projections are read-side components that build read models from events.
//! They are a key component of CQRS architecture and live within query slices.

use std::path::PathBuf;

/// Metadata for a projection (read model builder)
#[derive(Debug, Clone, PartialEq)]
pub struct Projection {
    /// Name of the projection (e.g., "WorkflowListProjection")
    pub name: String,

    /// File path relative to project root
    pub file_path: PathBuf,

    /// Events this projection subscribes to
    pub subscribed_events: Vec<String>,

    /// Read model type this projection builds
    pub read_model: Option<String>,

    /// Line count (for thin adapter validation)
    pub line_count: usize,
}

impl Projection {
    /// Check if this projection subscribes to a specific event
    pub fn subscribes_to(&self, event_name: &str) -> bool {
        self.subscribed_events.iter().any(|e| e == event_name || e.contains(event_name))
    }

    /// Check if this projection has any event subscriptions
    pub fn has_subscriptions(&self) -> bool {
        !self.subscribed_events.is_empty()
    }

    /// Get the inferred slice name from file path
    pub fn infer_slice_name(&self) -> Option<String> {
        // Extract slice name from path like slices/list_workflows/WorkflowListProjection.py
        self.file_path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
    }

    /// Check if this is a list projection (builds a list read model)
    pub fn is_list_projection(&self) -> bool {
        self.name.contains("List") || self.name.contains("All")
    }

    /// Check if this is a detail projection (builds a single item read model)
    pub fn is_detail_projection(&self) -> bool {
        self.name.contains("Detail") || self.name.contains("ById") || self.name.contains("Single")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_list_projection() -> Projection {
        Projection {
            name: "WorkflowListProjection".to_string(),
            file_path: PathBuf::from("slices/list_workflows/WorkflowListProjection.py"),
            subscribed_events: vec![
                "WorkflowCreatedEvent".to_string(),
                "WorkflowCompletedEvent".to_string(),
            ],
            read_model: Some("WorkflowSummary".to_string()),
            line_count: 45,
        }
    }

    fn create_detail_projection() -> Projection {
        Projection {
            name: "WorkflowDetailProjection".to_string(),
            file_path: PathBuf::from("slices/get_workflow_detail/WorkflowDetailProjection.py"),
            subscribed_events: vec![
                "WorkflowCreatedEvent".to_string(),
                "WorkflowPhaseStartedEvent".to_string(),
            ],
            read_model: Some("WorkflowDetail".to_string()),
            line_count: 60,
        }
    }

    #[test]
    fn test_subscribes_to() {
        let projection = create_list_projection();

        assert!(projection.subscribes_to("WorkflowCreatedEvent"));
        assert!(projection.subscribes_to("WorkflowCompleted")); // Partial match
        assert!(!projection.subscribes_to("SessionCreatedEvent"));
    }

    #[test]
    fn test_has_subscriptions() {
        let projection = create_list_projection();
        assert!(projection.has_subscriptions());

        let empty_projection = Projection {
            name: "EmptyProjection".to_string(),
            file_path: PathBuf::from("slices/empty/EmptyProjection.py"),
            subscribed_events: vec![],
            read_model: None,
            line_count: 10,
        };
        assert!(!empty_projection.has_subscriptions());
    }

    #[test]
    fn test_infer_slice_name() {
        let projection = create_list_projection();
        assert_eq!(projection.infer_slice_name(), Some("list_workflows".to_string()));

        let detail = create_detail_projection();
        assert_eq!(detail.infer_slice_name(), Some("get_workflow_detail".to_string()));
    }

    #[test]
    fn test_is_list_projection() {
        let list = create_list_projection();
        assert!(list.is_list_projection());

        let detail = create_detail_projection();
        assert!(!detail.is_list_projection());
    }

    #[test]
    fn test_is_detail_projection() {
        let detail = create_detail_projection();
        assert!(detail.is_detail_projection());

        let list = create_list_projection();
        assert!(!list.is_detail_projection());
    }
}
