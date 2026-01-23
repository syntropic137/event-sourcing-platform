//! Value object metadata

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Metadata for a value object
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ValueObject {
    /// Name of the value object
    pub name: String,

    /// File path
    pub file_path: PathBuf,

    /// Fields in the value object (if parseable)
    pub fields: Vec<ValueObjectField>,

    /// Whether this value object is immutable (frozen, readonly, etc.)
    pub is_immutable: bool,

    /// Line count
    pub line_count: usize,
}

/// Field within a value object
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ValueObjectField {
    /// Field name
    pub name: String,

    /// Field type (if parseable)
    pub field_type: Option<String>,

    /// Whether field is optional
    pub is_optional: bool,
}

impl ValueObject {
    /// Check if value object has a specific field
    pub fn has_field(&self, name: &str) -> bool {
        self.fields.iter().any(|f| f.name == name)
    }

    /// Get field by name
    pub fn get_field(&self, name: &str) -> Option<&ValueObjectField> {
        self.fields.iter().find(|f| f.name == name)
    }

    /// Get required fields
    pub fn required_fields(&self) -> Vec<&ValueObjectField> {
        self.fields.iter().filter(|f| !f.is_optional).collect()
    }

    /// Get optional fields
    pub fn optional_fields(&self) -> Vec<&ValueObjectField> {
        self.fields.iter().filter(|f| f.is_optional).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_field() {
        let vo = ValueObject {
            name: "EmailValueObjects".to_string(),
            file_path: PathBuf::from("domain/EmailValueObjects.py"),
            fields: vec![ValueObjectField {
                name: "email".to_string(),
                field_type: Some("str".to_string()),
                is_optional: false,
            }],
            is_immutable: true,
            line_count: 10,
        };

        assert!(vo.has_field("email"));
        assert!(!vo.has_field("nonexistent"));
    }

    #[test]
    fn test_required_optional_fields() {
        let vo = ValueObject {
            name: "PersonValueObjects".to_string(),
            file_path: PathBuf::from("domain/PersonValueObjects.py"),
            fields: vec![
                ValueObjectField {
                    name: "name".to_string(),
                    field_type: Some("str".to_string()),
                    is_optional: false,
                },
                ValueObjectField {
                    name: "age".to_string(),
                    field_type: Some("int".to_string()),
                    is_optional: true,
                },
            ],
            is_immutable: true,
            line_count: 20,
        };

        assert_eq!(vo.required_fields().len(), 1);
        assert_eq!(vo.optional_fields().len(), 1);
    }
}
