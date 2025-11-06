//! Framework integration support

use crate::config::FrameworkConfig;

/// Framework integration manager
#[derive(Debug)]
pub struct FrameworkIntegration {
    config: Option<FrameworkConfig>,
}

impl FrameworkIntegration {
    /// Create new framework integration
    pub fn new(config: Option<FrameworkConfig>) -> Self {
        Self { config }
    }

    /// Check if framework integration is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.is_some()
    }

    /// Get base type import for a given type
    pub fn get_base_type_import(&self, type_name: &str) -> Option<String> {
        self.config.as_ref().and_then(|c| c.base_types.get(type_name)).map(|bt| bt.import.clone())
    }

    /// Get base type class name for a given type
    pub fn get_base_type_class(&self, type_name: &str) -> Option<String> {
        self.config.as_ref().and_then(|c| c.base_types.get(type_name)).map(|bt| bt.class.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_framework() {
        let integration = FrameworkIntegration::new(None);
        assert!(!integration.is_enabled());
        assert!(integration.get_base_type_import("domain_event").is_none());
    }
}
