//! Command metadata

use std::path::PathBuf;

/// Metadata for a domain command
#[derive(Debug, Clone, PartialEq)]
pub struct Command {
    /// Name of the command (e.g., "CreateTaskCommand")
    pub name: String,

    /// File path relative to project root
    pub file_path: PathBuf,

    /// Whether the command has an aggregateId field
    pub has_aggregate_id: bool,

    /// Command fields/properties
    pub fields: Vec<CommandField>,
}

impl Command {
    /// Check if this command has a specific field
    pub fn has_field(&self, field_name: &str) -> bool {
        self.fields.iter().any(|f| f.name == field_name)
    }

    /// Get required fields
    pub fn required_fields(&self) -> Vec<&CommandField> {
        self.fields.iter().filter(|f| f.required).collect()
    }

    /// Get optional fields
    pub fn optional_fields(&self) -> Vec<&CommandField> {
        self.fields.iter().filter(|f| !f.required).collect()
    }
}

/// Metadata for a command field
#[derive(Debug, Clone, PartialEq)]
pub struct CommandField {
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

    fn create_test_command() -> Command {
        Command {
            name: "CreateTaskCommand".to_string(),
            file_path: PathBuf::from("domain/commands/CreateTaskCommand.ts"),
            has_aggregate_id: true,
            fields: vec![
                CommandField {
                    name: "aggregateId".to_string(),
                    field_type: "string".to_string(),
                    required: true,
                    line_number: 5,
                },
                CommandField {
                    name: "title".to_string(),
                    field_type: "string".to_string(),
                    required: true,
                    line_number: 6,
                },
                CommandField {
                    name: "description".to_string(),
                    field_type: "string".to_string(),
                    required: false,
                    line_number: 7,
                },
            ],
        }
    }

    #[test]
    fn test_has_field() {
        let command = create_test_command();

        assert!(command.has_field("aggregateId"));
        assert!(command.has_field("title"));
        assert!(command.has_field("description"));
        assert!(!command.has_field("nonExistent"));
    }

    #[test]
    fn test_required_fields() {
        let command = create_test_command();
        let required = command.required_fields();

        assert_eq!(required.len(), 2);
        assert!(required.iter().any(|f| f.name == "aggregateId"));
        assert!(required.iter().any(|f| f.name == "title"));
    }

    #[test]
    fn test_optional_fields() {
        let command = create_test_command();
        let optional = command.optional_fields();

        assert_eq!(optional.len(), 1);
        assert_eq!(optional[0].name, "description");
    }

    #[test]
    fn test_has_aggregate_id() {
        let command = create_test_command();
        assert!(command.has_aggregate_id);

        let command_without_id = Command {
            name: "SomeCommand".to_string(),
            file_path: PathBuf::from("domain/commands/SomeCommand.ts"),
            has_aggregate_id: false,
            fields: vec![],
        };
        assert!(!command_without_id.has_aggregate_id);
    }
}
