//! Query metadata

use std::path::PathBuf;

/// Metadata for a domain query
#[derive(Debug, Clone, PartialEq)]
pub struct Query {
    /// Name of the query (e.g., "GetTaskByIdQuery", "ListTasksQuery")
    pub name: String,

    /// File path relative to project root
    pub file_path: PathBuf,

    /// Query fields/parameters
    pub fields: Vec<QueryField>,
}

impl Query {
    /// Check if this query has a specific field
    pub fn has_field(&self, field_name: &str) -> bool {
        self.fields.iter().any(|f| f.name == field_name)
    }

    /// Get required fields
    pub fn required_fields(&self) -> Vec<&QueryField> {
        self.fields.iter().filter(|f| f.required).collect()
    }

    /// Get optional fields
    pub fn optional_fields(&self) -> Vec<&QueryField> {
        self.fields.iter().filter(|f| !f.required).collect()
    }

    /// Check if this is a list query (typically no required fields or has pagination)
    pub fn is_list_query(&self) -> bool {
        self.name.contains("List") || self.name.contains("GetAll")
    }

    /// Check if this is a get-by-id query
    pub fn is_get_by_id_query(&self) -> bool {
        self.name.contains("GetById")
            || self.name.contains("GetByAggregateId")
            || self.name.contains("GetBy")
            || self.name.contains("ById")
    }
}

/// Metadata for a query field
#[derive(Debug, Clone, PartialEq)]
pub struct QueryField {
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

    fn create_test_get_by_id_query() -> Query {
        Query {
            name: "GetTaskByIdQuery".to_string(),
            file_path: PathBuf::from("domain/queries/GetTaskByIdQuery.ts"),
            fields: vec![QueryField {
                name: "taskId".to_string(),
                field_type: "string".to_string(),
                required: true,
                line_number: 5,
            }],
        }
    }

    fn create_test_list_query() -> Query {
        Query {
            name: "ListTasksQuery".to_string(),
            file_path: PathBuf::from("domain/queries/ListTasksQuery.ts"),
            fields: vec![
                QueryField {
                    name: "page".to_string(),
                    field_type: "number".to_string(),
                    required: false,
                    line_number: 5,
                },
                QueryField {
                    name: "pageSize".to_string(),
                    field_type: "number".to_string(),
                    required: false,
                    line_number: 6,
                },
            ],
        }
    }

    #[test]
    fn test_has_field() {
        let query = create_test_get_by_id_query();

        assert!(query.has_field("taskId"));
        assert!(!query.has_field("nonExistent"));
    }

    #[test]
    fn test_required_fields() {
        let query = create_test_get_by_id_query();
        let required = query.required_fields();

        assert_eq!(required.len(), 1);
        assert_eq!(required[0].name, "taskId");
    }

    #[test]
    fn test_optional_fields() {
        let query = create_test_list_query();
        let optional = query.optional_fields();

        assert_eq!(optional.len(), 2);
        assert!(optional.iter().any(|f| f.name == "page"));
        assert!(optional.iter().any(|f| f.name == "pageSize"));
    }

    #[test]
    fn test_is_list_query() {
        let list_query = create_test_list_query();
        assert!(list_query.is_list_query());

        let get_by_id_query = create_test_get_by_id_query();
        assert!(!get_by_id_query.is_list_query());
    }

    #[test]
    fn test_is_get_by_id_query() {
        let get_by_id_query = create_test_get_by_id_query();
        assert!(get_by_id_query.is_get_by_id_query());

        let list_query = create_test_list_query();
        assert!(!list_query.is_get_by_id_query());
    }
}
