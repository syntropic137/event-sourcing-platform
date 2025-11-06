//! Pattern matching for VSA file types

use regex::Regex;
use std::path::Path;

use crate::config::PatternsConfig;

/// Pattern matcher for VSA file types
#[derive(Debug)]
pub struct PatternMatcher {
    patterns: PatternsConfig,
    #[allow(dead_code)]
    extension: String,
}

impl PatternMatcher {
    /// Create a new pattern matcher
    pub fn new(patterns: PatternsConfig, extension: String) -> Self {
        Self { patterns, extension }
    }

    /// Check if a file matches the command pattern
    pub fn is_command(&self, path: &Path) -> bool {
        self.matches_pattern(path, &self.patterns.command)
    }

    /// Check if a file matches the event pattern
    pub fn is_event(&self, path: &Path) -> bool {
        self.matches_pattern(path, &self.patterns.event)
    }

    /// Check if a file matches the handler pattern
    pub fn is_handler(&self, path: &Path) -> bool {
        self.matches_pattern(path, &self.patterns.handler)
    }

    /// Check if a file matches the query pattern
    pub fn is_query(&self, path: &Path) -> bool {
        self.matches_pattern(path, &self.patterns.query)
    }

    /// Check if a file matches the integration event pattern
    pub fn is_integration_event(&self, path: &Path) -> bool {
        self.matches_pattern(path, &self.patterns.integration_event)
    }

    /// Check if a file matches the test pattern
    pub fn is_test(&self, path: &Path) -> bool {
        self.matches_pattern(path, &self.patterns.test)
    }

    /// Get the file type
    pub fn get_file_type(&self, path: &Path) -> Option<FileType> {
        if self.is_command(path) {
            Some(FileType::Command)
        } else if self.is_integration_event(path) {
            Some(FileType::IntegrationEvent)
        } else if self.is_event(path) {
            Some(FileType::Event)
        } else if self.is_handler(path) {
            Some(FileType::Handler)
        } else if self.is_query(path) {
            Some(FileType::Query)
        } else if self.is_test(path) {
            Some(FileType::Test)
        } else {
            None
        }
    }

    fn matches_pattern(&self, path: &Path, pattern: &str) -> bool {
        let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");

        // Convert glob pattern to regex
        let regex_pattern = self.glob_to_regex(pattern);

        if let Ok(re) = Regex::new(&regex_pattern) {
            re.is_match(file_stem)
        } else {
            false
        }
    }

    fn glob_to_regex(&self, pattern: &str) -> String {
        pattern.replace(".", r"\.").replace("*", ".*").replace("?", ".")
    }
}

/// VSA file type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    Command,
    Event,
    IntegrationEvent,
    Handler,
    Query,
    Test,
}

impl FileType {
    /// Get the file type name
    pub fn name(&self) -> &'static str {
        match self {
            FileType::Command => "Command",
            FileType::Event => "Event",
            FileType::IntegrationEvent => "IntegrationEvent",
            FileType::Handler => "Handler",
            FileType::Query => "Query",
            FileType::Test => "Test",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn create_matcher() -> PatternMatcher {
        // Use simple patterns for testing (not the v2 defaults with **/)
        let config = PatternsConfig {
            command: "*Command".to_string(),
            event: "*Event".to_string(),
            handler: "*Handler".to_string(),
            query: "*Query".to_string(),
            integration_event: "*IntegrationEvent".to_string(),
            test: "*.test".to_string(),
        };
        PatternMatcher::new(config, "ts".to_string())
    }

    #[test]
    fn test_command_pattern() {
        let matcher = create_matcher();

        assert!(matcher.is_command(&PathBuf::from("CreateProductCommand.ts")));
        assert!(matcher.is_command(&PathBuf::from("UpdateInventoryCommand.ts")));
        assert!(!matcher.is_command(&PathBuf::from("ProductCreatedEvent.ts")));
    }

    #[test]
    fn test_event_pattern() {
        let matcher = create_matcher();

        assert!(matcher.is_event(&PathBuf::from("ProductCreatedEvent.ts")));
        assert!(matcher.is_event(&PathBuf::from("StockAdjustedEvent.ts")));
        assert!(!matcher.is_event(&PathBuf::from("CreateProductCommand.ts")));
    }

    #[test]
    fn test_integration_event_pattern() {
        let matcher = create_matcher();

        assert!(matcher.is_integration_event(&PathBuf::from("OrderPlacedIntegrationEvent.ts")));
        assert!(!matcher.is_integration_event(&PathBuf::from("OrderPlacedEvent.ts")));
    }

    #[test]
    fn test_get_file_type() {
        let matcher = create_matcher();

        assert_eq!(
            matcher.get_file_type(&PathBuf::from("CreateProductCommand.ts")),
            Some(FileType::Command)
        );
        assert_eq!(
            matcher.get_file_type(&PathBuf::from("ProductCreatedEvent.ts")),
            Some(FileType::Event)
        );
        assert_eq!(
            matcher.get_file_type(&PathBuf::from("CreateProductHandler.ts")),
            Some(FileType::Handler)
        );
        assert_eq!(matcher.get_file_type(&PathBuf::from("random-file.ts")), None);
    }

    #[test]
    fn test_v2_config_defaults() {
        // Test that v2 config has the correct defaults
        use crate::config::{DomainConfig, SlicesConfig};

        let domain_config = DomainConfig::default();
        assert_eq!(domain_config.path, PathBuf::from("domain"));
        assert_eq!(domain_config.aggregates.pattern, "*Aggregate.*");
        assert_eq!(domain_config.commands.pattern, "**/*Command.*");
        assert_eq!(domain_config.events.pattern, "**/*Event.*");

        let slices_config = SlicesConfig::default();
        assert_eq!(slices_config.path, PathBuf::from("slices"));
        assert_eq!(slices_config.types.len(), 3);
    }
}
