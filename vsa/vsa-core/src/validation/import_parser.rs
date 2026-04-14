//! Import statement parsing for dependency validation
//!
//! This module provides parsers for extracting import statements from
//! Python, TypeScript, and Rust source files to enable dependency validation.

use std::path::{Path, PathBuf};

use crate::error::Result;

/// Represents an import statement from a source file
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ImportStatement {
    /// The module/package being imported
    pub module: String,
    /// Whether this is a relative import
    pub is_relative: bool,
    /// Original line for reference
    pub line: String,
    /// Line number (1-indexed)
    pub line_number: usize,
}

/// Import parser for different languages
pub trait ImportParser {
    /// Parse imports from a file
    fn parse_file(&self, path: &Path) -> Result<Vec<ImportStatement>>;

    /// Parse imports from source code
    fn parse_source(&self, source: &str) -> Vec<ImportStatement>;
}

/// Python import parser
pub struct PythonImportParser;

impl PythonImportParser {
    pub fn new() -> Self {
        Self
    }

    /// Extract module path from Python import statement
    fn extract_module(line: &str) -> Option<String> {
        let trimmed = line.trim();

        // Handle "from X import Y" style
        if let Some(from_pos) = trimmed.find("from ") {
            let after_from = &trimmed[from_pos + 5..];
            if let Some(import_pos) = after_from.find(" import ") {
                return Some(after_from[..import_pos].trim().to_string());
            }
        }

        // Handle "import X" style
        if let Some(import_pos) = trimmed.find("import ") {
            let after_import = &trimmed[import_pos + 7..];
            // Take first module (before comma or as)
            let module = after_import.split(&[',', ' ', '\t'][..]).next()?.trim();
            if !module.is_empty() {
                return Some(module.to_string());
            }
        }

        None
    }

    /// Check if import is relative (starts with dots)
    fn is_relative(module: &str) -> bool {
        module.starts_with('.')
    }
}

impl ImportParser for PythonImportParser {
    fn parse_file(&self, path: &Path) -> Result<Vec<ImportStatement>> {
        let source = std::fs::read_to_string(path)?;
        Ok(self.parse_source(&source))
    }

    fn parse_source(&self, source: &str) -> Vec<ImportStatement> {
        let mut imports = Vec::new();
        let mut in_type_checking_block = false;
        let mut type_checking_indent: Option<usize> = None;

        for (line_num, line) in source.lines().enumerate() {
            let trimmed = line.trim();

            // Skip comments and empty lines
            if trimmed.starts_with('#') || trimmed.is_empty() {
                continue;
            }

            // Detect `if TYPE_CHECKING:` blocks
            if trimmed == "if TYPE_CHECKING:"
                || trimmed == "if typing.TYPE_CHECKING:"
            {
                in_type_checking_block = true;
                // Record the indentation level of the `if` statement
                let indent = line.len() - line.trim_start().len();
                type_checking_indent = Some(indent);
                continue;
            }

            // If inside a TYPE_CHECKING block, check if we've dedented back out
            if in_type_checking_block {
                if let Some(base_indent) = type_checking_indent {
                    let current_indent = line.len() - line.trim_start().len();
                    if current_indent <= base_indent {
                        // We've left the TYPE_CHECKING block
                        in_type_checking_block = false;
                        type_checking_indent = None;
                    } else {
                        // Still inside TYPE_CHECKING - skip this import
                        continue;
                    }
                }
            }

            // Check for import statements
            if trimmed.starts_with("from ") || trimmed.starts_with("import ") {
                if let Some(module) = Self::extract_module(line) {
                    imports.push(ImportStatement {
                        is_relative: Self::is_relative(&module),
                        module,
                        line: line.to_string(),
                        line_number: line_num + 1,
                    });
                }
            }
        }

        imports
    }
}

/// TypeScript import parser
pub struct TypeScriptImportParser;

impl TypeScriptImportParser {
    pub fn new() -> Self {
        Self
    }

    /// Extract module path from TypeScript import statement
    fn extract_module(line: &str) -> Option<String> {
        let trimmed = line.trim();

        // Find the 'from' keyword
        if let Some(from_pos) = trimmed.rfind(" from ") {
            let after_from = &trimmed[from_pos + 6..].trim();

            // Extract the module path from quotes
            if let Some(start) = after_from.find(&['\'', '"'][..]) {
                let quote_char = after_from.chars().nth(start)?;
                let module_start = start + 1;
                if let Some(end) = after_from[module_start..].find(quote_char) {
                    return Some(after_from[module_start..module_start + end].to_string());
                }
            }
        }

        // Handle require statements: const X = require('module')
        if trimmed.contains("require(") {
            if let Some(start_pos) = trimmed.find("require(") {
                let after_require = &trimmed[start_pos + 8..];
                if let Some(quote_start) = after_require.find(&['\'', '"'][..]) {
                    let quote_char = after_require.chars().nth(quote_start)?;
                    let module_start = quote_start + 1;
                    if let Some(end) = after_require[module_start..].find(quote_char) {
                        return Some(after_require[module_start..module_start + end].to_string());
                    }
                }
            }
        }

        None
    }

    /// Check if import is relative (starts with ./ or ../)
    fn is_relative(module: &str) -> bool {
        module.starts_with("./") || module.starts_with("../")
    }
}

impl ImportParser for TypeScriptImportParser {
    fn parse_file(&self, path: &Path) -> Result<Vec<ImportStatement>> {
        let source = std::fs::read_to_string(path)?;
        Ok(self.parse_source(&source))
    }

    fn parse_source(&self, source: &str) -> Vec<ImportStatement> {
        let mut imports = Vec::new();

        for (line_num, line) in source.lines().enumerate() {
            let trimmed = line.trim();

            // Skip comments and empty lines
            if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.is_empty() {
                continue;
            }

            // Check for import statements
            if trimmed.starts_with("import ")
                || trimmed.contains(" from ")
                || trimmed.contains("require(")
            {
                if let Some(module) = Self::extract_module(line) {
                    imports.push(ImportStatement {
                        is_relative: Self::is_relative(&module),
                        module,
                        line: line.to_string(),
                        line_number: line_num + 1,
                    });
                }
            }
        }

        imports
    }
}

/// Rust import parser
pub struct RustImportParser;

impl RustImportParser {
    pub fn new() -> Self {
        Self
    }

    /// Extract module path from Rust use statement
    fn extract_module(line: &str) -> Option<String> {
        let trimmed = line.trim();

        // Handle "use" statements
        if let Some(use_pos) = trimmed.find("use ") {
            let after_use = &trimmed[use_pos + 4..];

            // Find the end (semicolon, as, {)
            let end_chars = &[';', '{'][..];
            let end_pos =
                after_use.find(|c: char| end_chars.contains(&c)).unwrap_or(after_use.len());

            // Also check for " as "
            let as_pos = after_use[..end_pos].find(" as ").unwrap_or(end_pos);

            let module = after_use[..as_pos.min(end_pos)].trim();

            if !module.is_empty() {
                return Some(module.to_string());
            }
        }

        None
    }

    /// Check if import is relative (starts with self, super, or crate)
    fn is_relative(module: &str) -> bool {
        module.starts_with("self::")
            || module.starts_with("super::")
            || module.starts_with("crate::")
    }
}

impl ImportParser for RustImportParser {
    fn parse_file(&self, path: &Path) -> Result<Vec<ImportStatement>> {
        let source = std::fs::read_to_string(path)?;
        Ok(self.parse_source(&source))
    }

    fn parse_source(&self, source: &str) -> Vec<ImportStatement> {
        let mut imports = Vec::new();

        for (line_num, line) in source.lines().enumerate() {
            let trimmed = line.trim();

            // Skip comments and empty lines
            if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.is_empty() {
                continue;
            }

            // Check for use statements
            if trimmed.starts_with("use ") {
                if let Some(module) = Self::extract_module(line) {
                    imports.push(ImportStatement {
                        is_relative: Self::is_relative(&module),
                        module,
                        line: line.to_string(),
                        line_number: line_num + 1,
                    });
                }
            }
        }

        imports
    }
}

/// Get appropriate parser for a file based on extension
pub fn get_parser(path: &Path) -> Option<Box<dyn ImportParser>> {
    let extension = path.extension()?.to_str()?;

    match extension {
        "py" => Some(Box::new(PythonImportParser::new())),
        "ts" | "tsx" | "js" | "jsx" => Some(Box::new(TypeScriptImportParser::new())),
        "rs" => Some(Box::new(RustImportParser::new())),
        _ => None,
    }
}

/// Parse all imports from a file
pub fn parse_imports(path: &Path) -> Result<Vec<ImportStatement>> {
    if let Some(parser) = get_parser(path) {
        parser.parse_file(path)
    } else {
        Ok(Vec::new())
    }
}

/// Resolve import path relative to file location
#[allow(dead_code)] // Reserved for future use
pub fn resolve_import_path(
    file_path: &Path,
    import: &ImportStatement,
    context_root: &Path,
) -> Option<PathBuf> {
    if import.is_relative {
        // Relative import - resolve from file's directory
        let file_dir = file_path.parent()?;
        let mut resolved = file_dir.to_path_buf();

        // Handle Python relative imports (dots)
        if import.module.starts_with('.') {
            let dot_count = import.module.chars().take_while(|&c| c == '.').count();
            for _ in 1..dot_count {
                resolved = resolved.parent()?.to_path_buf();
            }
            let module_path = import.module.trim_start_matches('.');
            if !module_path.is_empty() {
                resolved.push(module_path.replace('.', "/"));
            }
        } else {
            // TypeScript/Rust relative imports
            resolved.push(&import.module);
        }

        Some(resolved)
    } else {
        // Absolute import - resolve from context root
        let module_path = import.module.replace('.', "/").replace("::", "/");
        Some(context_root.join(module_path))
    }
}

/// Determine which architectural layer a file belongs to
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchitectureLayer {
    Domain,
    Events,
    Ports,
    Application,
    Infrastructure,
    Slices,
    Unknown,
}

/// Detect the architecture layer from a file path
pub fn detect_layer(path: &Path, context_root: &Path) -> ArchitectureLayer {
    if let Ok(relative) = path.strip_prefix(context_root) {
        let components: Vec<_> = relative.components().collect();
        if components.is_empty() {
            return ArchitectureLayer::Unknown;
        }

        let first_component = components[0].as_os_str().to_string_lossy();

        match first_component.as_ref() {
            "domain" => ArchitectureLayer::Domain,
            "events" => ArchitectureLayer::Events,
            "ports" => ArchitectureLayer::Ports,
            "application" => ArchitectureLayer::Application,
            "infrastructure" => ArchitectureLayer::Infrastructure,
            "slices" => ArchitectureLayer::Slices,
            _ => ArchitectureLayer::Unknown,
        }
    } else {
        ArchitectureLayer::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // PYTHON IMPORT PARSER TESTS
    // ========================================================================

    #[test]
    fn test_python_from_import() {
        let parser = PythonImportParser::new();
        let source = "from domain.WorkflowAggregate import WorkflowAggregate";
        let imports = parser.parse_source(source);

        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].module, "domain.WorkflowAggregate");
        assert!(!imports[0].is_relative);
    }

    #[test]
    fn test_python_relative_import() {
        let parser = PythonImportParser::new();
        let source = "from ..domain import WorkflowAggregate";
        let imports = parser.parse_source(source);

        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].module, "..domain");
        assert!(imports[0].is_relative);
    }

    #[test]
    fn test_python_simple_import() {
        let parser = PythonImportParser::new();
        let source = "import os";
        let imports = parser.parse_source(source);

        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].module, "os");
        assert!(!imports[0].is_relative);
    }

    #[test]
    fn test_python_multiple_imports() {
        let parser = PythonImportParser::new();
        let source = r#"
from domain.WorkflowAggregate import WorkflowAggregate
import os
from ..events import WorkflowCreatedEvent
"#;
        let imports = parser.parse_source(source);

        assert_eq!(imports.len(), 3);
        assert_eq!(imports[0].module, "domain.WorkflowAggregate");
        assert_eq!(imports[1].module, "os");
        assert_eq!(imports[2].module, "..events");
    }

    #[test]
    fn test_python_skip_comments() {
        let parser = PythonImportParser::new();
        let source = r#"
# from domain import Foo
from domain.WorkflowAggregate import WorkflowAggregate
"#;
        let imports = parser.parse_source(source);

        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].module, "domain.WorkflowAggregate");
    }

    #[test]
    fn test_python_type_checking_block_excluded() {
        let parser = PythonImportParser::new();
        let source = r#"
from __future__ import annotations

from typing import TYPE_CHECKING

from event_sourcing.core.checkpoint import CheckpointedProjection

if TYPE_CHECKING:
    from httpx import AsyncClient
    from boto3 import Session

from datetime import datetime
"#;
        let imports = parser.parse_source(source);

        // Should have 4 imports: __future__, typing, event_sourcing, datetime
        // httpx and boto3 are inside TYPE_CHECKING and should be excluded
        assert_eq!(imports.len(), 4);
        assert_eq!(imports[0].module, "__future__");
        assert_eq!(imports[1].module, "typing");
        assert_eq!(imports[2].module, "event_sourcing.core.checkpoint");
        assert_eq!(imports[3].module, "datetime");
    }

    #[test]
    fn test_python_type_checking_block_with_typing_prefix() {
        let parser = PythonImportParser::new();
        let source = r#"
import typing

if typing.TYPE_CHECKING:
    from httpx import AsyncClient

from uuid import uuid4
"#;
        let imports = parser.parse_source(source);

        assert_eq!(imports.len(), 2);
        assert_eq!(imports[0].module, "typing");
        assert_eq!(imports[1].module, "uuid");
    }

    // ========================================================================
    // TYPESCRIPT IMPORT PARSER TESTS
    // ========================================================================

    #[test]
    fn test_typescript_import_from() {
        let parser = TypeScriptImportParser::new();
        let source = "import { WorkflowAggregate } from './domain/WorkflowAggregate';";
        let imports = parser.parse_source(source);

        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].module, "./domain/WorkflowAggregate");
        assert!(imports[0].is_relative);
    }

    #[test]
    fn test_typescript_absolute_import() {
        let parser = TypeScriptImportParser::new();
        let source = "import { Something } from '@/domain/WorkflowAggregate';";
        let imports = parser.parse_source(source);

        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].module, "@/domain/WorkflowAggregate");
        assert!(!imports[0].is_relative);
    }

    #[test]
    fn test_typescript_require() {
        let parser = TypeScriptImportParser::new();
        let source = "const WorkflowAggregate = require('./domain/WorkflowAggregate');";
        let imports = parser.parse_source(source);

        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].module, "./domain/WorkflowAggregate");
        assert!(imports[0].is_relative);
    }

    #[test]
    fn test_typescript_parent_import() {
        let parser = TypeScriptImportParser::new();
        let source = "import { WorkflowAggregate } from '../domain/WorkflowAggregate';";
        let imports = parser.parse_source(source);

        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].module, "../domain/WorkflowAggregate");
        assert!(imports[0].is_relative);
    }

    #[test]
    fn test_typescript_skip_comments() {
        let parser = TypeScriptImportParser::new();
        let source = r#"
// import { Foo } from './domain/Foo';
import { WorkflowAggregate } from './domain/WorkflowAggregate';
"#;
        let imports = parser.parse_source(source);

        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].module, "./domain/WorkflowAggregate");
    }

    // ========================================================================
    // RUST IMPORT PARSER TESTS
    // ========================================================================

    #[test]
    fn test_rust_use_statement() {
        let parser = RustImportParser::new();
        let source = "use crate::domain::WorkflowAggregate;";
        let imports = parser.parse_source(source);

        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].module, "crate::domain::WorkflowAggregate");
        assert!(imports[0].is_relative);
    }

    #[test]
    fn test_rust_use_super() {
        let parser = RustImportParser::new();
        let source = "use super::domain::WorkflowAggregate;";
        let imports = parser.parse_source(source);

        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].module, "super::domain::WorkflowAggregate");
        assert!(imports[0].is_relative);
    }

    #[test]
    fn test_rust_use_external() {
        let parser = RustImportParser::new();
        let source = "use std::collections::HashMap;";
        let imports = parser.parse_source(source);

        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].module, "std::collections::HashMap");
        assert!(!imports[0].is_relative);
    }

    #[test]
    fn test_rust_use_with_braces() {
        let parser = RustImportParser::new();
        let source = "use crate::domain::{WorkflowAggregate, WorkflowExecutionAggregate};";
        let imports = parser.parse_source(source);

        assert_eq!(imports.len(), 1);
        // Module includes the :: before the braces
        assert_eq!(imports[0].module, "crate::domain::");
    }

    #[test]
    fn test_rust_use_as() {
        let parser = RustImportParser::new();
        let source = "use crate::domain::WorkflowAggregate as WA;";
        let imports = parser.parse_source(source);

        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].module, "crate::domain::WorkflowAggregate");
    }

    // ========================================================================
    // LAYER DETECTION TESTS
    // ========================================================================

    #[test]
    fn test_detect_domain_layer() {
        let context_root = PathBuf::from("/app/contexts/workflows");
        let file_path = context_root.join("domain/WorkflowAggregate.py");

        let layer = detect_layer(&file_path, &context_root);
        assert_eq!(layer, ArchitectureLayer::Domain);
    }

    #[test]
    fn test_detect_events_layer() {
        let context_root = PathBuf::from("/app/contexts/workflows");
        let file_path = context_root.join("events/WorkflowCreatedEvent.py");

        let layer = detect_layer(&file_path, &context_root);
        assert_eq!(layer, ArchitectureLayer::Events);
    }

    #[test]
    fn test_detect_slices_layer() {
        let context_root = PathBuf::from("/app/contexts/workflows");
        let file_path = context_root.join("slices/create_workflow/internal/Handler.py");

        let layer = detect_layer(&file_path, &context_root);
        assert_eq!(layer, ArchitectureLayer::Slices);
    }

    #[test]
    fn test_get_parser_python() {
        let path = PathBuf::from("test.py");
        assert!(get_parser(&path).is_some());
    }

    #[test]
    fn test_get_parser_typescript() {
        let path = PathBuf::from("test.ts");
        assert!(get_parser(&path).is_some());
    }

    #[test]
    fn test_get_parser_rust() {
        let path = PathBuf::from("test.rs");
        assert!(get_parser(&path).is_some());
    }

    #[test]
    fn test_get_parser_unknown() {
        let path = PathBuf::from("test.txt");
        assert!(get_parser(&path).is_none());
    }
}
