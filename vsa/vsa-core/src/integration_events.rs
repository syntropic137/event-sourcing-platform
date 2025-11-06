//! Integration event validation and duplication detection

use crate::config::VsaConfig;
use crate::error::Result;
use crate::patterns::PatternMatcher;
use crate::scanner::Scanner;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Integration event information
#[derive(Debug, Clone)]
pub struct IntegrationEvent {
    pub name: String,
    pub path: PathBuf,
    pub publisher: String,
}

/// Integration event registry for tracking events across contexts
#[derive(Debug)]
pub struct IntegrationEventRegistry {
    events: HashMap<String, Vec<IntegrationEvent>>,
}

impl IntegrationEventRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self { events: HashMap::new() }
    }

    /// Scan and register all integration events
    pub fn scan(config: &VsaConfig, root: &Path) -> Result<Self> {
        let mut registry = Self::new();
        let scanner = Scanner::new(config.clone(), root.to_path_buf());
        let pattern_matcher =
            PatternMatcher::new(config.patterns.clone(), config.file_extension().to_string());

        let contexts = scanner.scan_contexts()?;

        for context in contexts {
            let shared_path = context.path.join("_shared").join("integration-events");

            if shared_path.exists() {
                let files = scanner.scan_feature_files(&shared_path)?;

                for file in files {
                    if pattern_matcher.is_integration_event(&file.path) {
                        let event_name =
                            file.path.file_stem().unwrap().to_string_lossy().to_string();

                        registry.register(IntegrationEvent {
                            name: event_name,
                            path: file.path.clone(),
                            publisher: context.name.clone(),
                        });
                    }
                }
            }
        }

        // Also check global _shared/integration-events
        let global_shared = root.join("_shared").join("integration-events");
        if global_shared.exists() {
            let files = scanner.scan_feature_files(&global_shared)?;

            for file in files {
                if pattern_matcher.is_integration_event(&file.path) {
                    let event_name = file.path.file_stem().unwrap().to_string_lossy().to_string();

                    registry.register(IntegrationEvent {
                        name: event_name,
                        path: file.path.clone(),
                        publisher: "_shared".to_string(),
                    });
                }
            }
        }

        Ok(registry)
    }

    /// Register an integration event
    pub fn register(&mut self, event: IntegrationEvent) {
        self.events.entry(event.name.clone()).or_default().push(event);
    }

    /// Find duplicates
    pub fn find_duplicates(&self) -> Vec<(String, Vec<String>)> {
        self.events
            .iter()
            .filter(|(_, events)| events.len() > 1)
            .map(|(name, events)| {
                let publishers: Vec<String> = events.iter().map(|e| e.publisher.clone()).collect();
                (name.clone(), publishers)
            })
            .collect()
    }

    /// Get all events published by a context
    pub fn get_published_by(&self, context: &str) -> Vec<&IntegrationEvent> {
        self.events.values().flatten().filter(|e| e.publisher == context).collect()
    }

    /// Check if an event exists
    pub fn has_event(&self, event_name: &str) -> bool {
        self.events.contains_key(event_name)
    }

    /// Get all registered events
    pub fn all_events(&self) -> Vec<&IntegrationEvent> {
        self.events.values().flatten().collect()
    }
}

impl Default for IntegrationEventRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_creation() {
        let registry = IntegrationEventRegistry::new();
        assert!(registry.all_events().is_empty());
    }

    #[test]
    fn test_register_event() {
        let mut registry = IntegrationEventRegistry::new();

        registry.register(IntegrationEvent {
            name: "OrderPlaced".to_string(),
            path: PathBuf::from("/test/OrderPlacedIntegrationEvent.ts"),
            publisher: "sales".to_string(),
        });

        assert!(registry.has_event("OrderPlaced"));
        assert_eq!(registry.all_events().len(), 1);
    }

    #[test]
    fn test_find_duplicates() {
        let mut registry = IntegrationEventRegistry::new();

        registry.register(IntegrationEvent {
            name: "OrderPlaced".to_string(),
            path: PathBuf::from("/contexts/sales/OrderPlacedIntegrationEvent.ts"),
            publisher: "sales".to_string(),
        });

        registry.register(IntegrationEvent {
            name: "OrderPlaced".to_string(),
            path: PathBuf::from("/contexts/warehouse/OrderPlacedIntegrationEvent.ts"),
            publisher: "warehouse".to_string(),
        });

        let duplicates = registry.find_duplicates();
        assert_eq!(duplicates.len(), 1);
        assert_eq!(duplicates[0].0, "OrderPlaced");
        assert_eq!(duplicates[0].1.len(), 2);
    }
}
