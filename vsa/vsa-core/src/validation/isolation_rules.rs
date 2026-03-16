//! Slice isolation validation rules
//!
//! Validates that slices remain isolated from each other:
//! - VSA010: No cross-slice imports (slices cannot import from sibling slices)
//! - VSA011: Thin adapter validation (controllers/adapters should be small)

use super::{EnhancedValidationReport, Severity, Suggestion, ValidationContext, ValidationIssue};
use crate::error::Result;
use crate::scanners::SliceScanner;
use regex::Regex;
use std::fs;

use super::rules::ValidationRule;

/// VSA010: No cross-slice imports
///
/// Slices should be isolated from each other. They can only import from:
/// - domain/ (commands, queries, events, aggregates)
/// - infrastructure/ (repositories, services)
/// - External packages
///
/// They should NOT import from sibling slices.
pub struct NoCrossSliceImportsRule;

impl ValidationRule for NoCrossSliceImportsRule {
    fn name(&self) -> &str {
        "no-cross-slice-imports"
    }

    fn code(&self) -> &str {
        "VSA010"
    }

    fn validate(
        &self,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        // Check if architecture validation is enabled
        let enforce =
            ctx.config.validation.architecture.as_ref().map(|a| a.slices_isolated).unwrap_or(true);

        if !enforce {
            return Ok(());
        }

        // Get slices config
        let slices_config = ctx.config.slices.as_ref();

        // Scan for slices
        let scanner = SliceScanner::new(slices_config, &ctx.root);
        let slices = scanner.scan()?;

        // Collect all slice names for cross-reference
        let slice_names: Vec<&str> = slices.iter().map(|s| s.name.as_str()).collect();

        for slice in &slices {
            for file in &slice.files {
                // Skip non-source files
                if !is_source_file(&file.name) {
                    continue;
                }

                // Skip test files and conftest (test infrastructure may legitimately cross slice boundaries)
                if is_test_or_conftest_name(&file.name) {
                    continue;
                }

                // Read file and check for cross-slice imports
                if let Ok(content) = fs::read_to_string(&file.path) {
                    let violations = detect_cross_slice_imports(
                        &content,
                        &slice.name,
                        &slice_names,
                        &ctx.config.language,
                    );

                    for violation in violations {
                        report.errors.push(ValidationIssue {
                            path: file.path.clone(),
                            code: self.code().to_string(),
                            severity: Severity::Error,
                            message: format!(
                                "Cross-slice import detected: '{}' imports from sibling slice '{}'. Slices must be isolated.",
                                file.name, violation.imported_slice
                            ),
                            suggestions: vec![Suggestion::manual(
                                format!(
                                    "Move shared code to domain/ or infrastructure/. Import path: {}",
                                    violation.import_path
                                )
                            )],
                        });
                    }
                }
            }
        }

        Ok(())
    }
}

/// VSA011: Thin adapter validation
///
/// Controllers and adapters should be thin - they should only:
/// - Parse requests
/// - Call the appropriate handler via bus
/// - Format responses
///
/// Business logic belongs in the domain layer, not in adapters.
pub struct ThinAdapterRule;

impl ValidationRule for ThinAdapterRule {
    fn name(&self) -> &str {
        "thin-adapter"
    }

    fn code(&self) -> &str {
        "VSA011"
    }

    fn validate(
        &self,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        // Check if thin adapter enforcement is enabled
        let enforce =
            ctx.config.validation.slices.as_ref().map(|s| s.enforce_thin_adapters).unwrap_or(true);

        if !enforce {
            return Ok(());
        }

        // Get max lines configuration
        let command_max_lines = ctx
            .config
            .slices
            .as_ref()
            .and_then(|s| s.command.as_ref())
            .and_then(|c| c.max_lines)
            .unwrap_or(50);

        let query_max_lines = ctx
            .config
            .slices
            .as_ref()
            .and_then(|s| s.query.as_ref())
            .and_then(|q| q.max_lines)
            .unwrap_or(100);

        // Get slices config
        let slices_config = ctx.config.slices.as_ref();

        // Scan for slices
        let scanner = SliceScanner::new(slices_config, &ctx.root);
        let slices = scanner.scan()?;

        for slice in &slices {
            let max_lines = match slice.slice_type {
                crate::config::SliceType::Command => command_max_lines,
                crate::config::SliceType::Query => query_max_lines,
                crate::config::SliceType::Saga => command_max_lines, // Use command default for sagas
                crate::config::SliceType::Mixed => command_max_lines, // Use command default for mixed slices
                crate::config::SliceType::Unknown => command_max_lines, // Use command default for unknown slices
            };

            for file in &slice.files {
                // Only check controllers/adapters
                if !is_adapter_file(&file.name) {
                    continue;
                }

                // Count lines (excluding blank lines and comments)
                if let Ok(content) = fs::read_to_string(&file.path) {
                    let line_count = count_code_lines(&content, &ctx.config.language);

                    if line_count > max_lines {
                        report.warnings.push(ValidationIssue {
                            path: file.path.clone(),
                            code: self.code().to_string(),
                            severity: Severity::Warning,
                            message: format!(
                                "Adapter '{}' has {} lines, exceeding the {} line limit. Adapters should be thin translation layers.",
                                file.name, line_count, max_lines
                            ),
                            suggestions: vec![Suggestion::manual(
                                "Move business logic to the handler or domain layer"
                            )],
                        });
                    }
                }
            }
        }

        Ok(())
    }
}

/// Import violation details
struct ImportViolation {
    imported_slice: String,
    import_path: String,
}

/// Test files and pytest conftest are test infrastructure —
/// they legitimately cross slice boundaries for integration testing.
fn is_test_or_conftest_name(name: &str) -> bool {
    name.starts_with("test_")
        || name.ends_with("_test.py")
        || name.ends_with("_test.rs")
        || name.ends_with(".test.ts")
        || name.ends_with(".test.tsx")
        || name.ends_with(".spec.ts")
        || name.ends_with(".spec.tsx")
        || name == "conftest.py"
}

/// Check if a file is a source file
fn is_source_file(name: &str) -> bool {
    name.ends_with(".ts")
        || name.ends_with(".tsx")
        || name.ends_with(".py")
        || name.ends_with(".rs")
}

/// Check if a file is an adapter/controller
fn is_adapter_file(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.contains("controller") || lower.contains("adapter") || lower.contains("route")
}

/// Detect cross-slice imports in file content
fn detect_cross_slice_imports(
    content: &str,
    current_slice: &str,
    all_slices: &[&str],
    language: &str,
) -> Vec<ImportViolation> {
    let mut violations = Vec::new();

    // Language-specific import patterns
    let imports = match language {
        "typescript" => extract_typescript_imports(content),
        "python" => extract_python_imports(content),
        "rust" => extract_rust_imports(content),
        _ => Vec::new(),
    };

    for import_path in imports {
        // Check if import references another slice
        for slice_name in all_slices {
            if *slice_name != current_slice {
                // Check various import patterns that might reference slices
                let patterns = [
                    format!("/{slice_name}/"),       // /slice_name/
                    format!("/{slice_name}"),        // /slice_name at end
                    format!("../{slice_name}/"),     // ../slice_name/
                    format!("slices/{slice_name}/"), // slices/slice_name/
                    format!("slices.{slice_name}"),  // slices.slice_name (Python)
                    format!("from {slice_name} "),   // from slice_name
                ];

                for pattern in &patterns {
                    if import_path.contains(pattern) {
                        violations.push(ImportViolation {
                            imported_slice: (*slice_name).to_string(),
                            import_path: import_path.clone(),
                        });
                        break;
                    }
                }
            }
        }
    }

    violations
}

/// Extract TypeScript/JavaScript imports
fn extract_typescript_imports(content: &str) -> Vec<String> {
    let mut imports = Vec::new();

    // Pattern: import ... from '...'
    let import_pattern = Regex::new(r#"import\s+.*?\s+from\s+['"]([^'"]+)['"]"#).unwrap();
    for cap in import_pattern.captures_iter(content) {
        if let Some(path) = cap.get(1) {
            imports.push(path.as_str().to_string());
        }
    }

    // Pattern: require('...')
    let require_pattern = Regex::new(r#"require\s*\(\s*['"]([^'"]+)['"]\s*\)"#).unwrap();
    for cap in require_pattern.captures_iter(content) {
        if let Some(path) = cap.get(1) {
            imports.push(path.as_str().to_string());
        }
    }

    imports
}

/// Extract Python imports
fn extract_python_imports(content: &str) -> Vec<String> {
    let mut imports = Vec::new();

    // Pattern: from x.y.z import ...
    let from_pattern = Regex::new(r#"from\s+([\w.]+)\s+import"#).unwrap();
    for cap in from_pattern.captures_iter(content) {
        if let Some(module) = cap.get(1) {
            imports.push(module.as_str().to_string());
        }
    }

    // Pattern: import x.y.z (use multiline flag to match at start of any line)
    let import_pattern = Regex::new(r#"(?m)^import\s+([\w.]+)"#).unwrap();
    for cap in import_pattern.captures_iter(content) {
        if let Some(module) = cap.get(1) {
            imports.push(module.as_str().to_string());
        }
    }

    imports
}

/// Extract Rust imports
fn extract_rust_imports(content: &str) -> Vec<String> {
    let mut imports = Vec::new();

    // Pattern: use crate::...
    let use_pattern = Regex::new(r#"use\s+(crate::[^;{]+)"#).unwrap();
    for cap in use_pattern.captures_iter(content) {
        if let Some(path) = cap.get(1) {
            imports.push(path.as_str().to_string());
        }
    }

    // Pattern: use super::...
    let super_pattern = Regex::new(r#"use\s+(super::[^;{]+)"#).unwrap();
    for cap in super_pattern.captures_iter(content) {
        if let Some(path) = cap.get(1) {
            imports.push(path.as_str().to_string());
        }
    }

    imports
}

/// Count non-blank, non-comment lines
fn count_code_lines(content: &str, language: &str) -> usize {
    let mut count = 0;
    let mut in_block_comment = false;

    for line in content.lines() {
        let trimmed = line.trim();

        // Skip empty lines
        if trimmed.is_empty() {
            continue;
        }

        // Handle block comments
        match language {
            "typescript" | "rust" => {
                if trimmed.starts_with("/*") {
                    in_block_comment = true;
                    if trimmed.ends_with("*/") {
                        in_block_comment = false;
                    }
                    continue;
                }
                if trimmed.ends_with("*/") {
                    in_block_comment = false;
                    continue;
                }
                if in_block_comment {
                    continue;
                }
                // Skip single-line comments
                if trimmed.starts_with("//") {
                    continue;
                }
            }
            "python" => {
                // Handle docstrings (simplified)
                if trimmed.starts_with("\"\"\"") || trimmed.starts_with("'''") {
                    // Check if this is a single-line docstring (opens and closes on same line)
                    // Need length check to avoid panic when slicing
                    if trimmed.len() > 3 {
                        let rest = &trimmed[3..];
                        if !rest.contains("\"\"\"") && !rest.contains("'''") {
                            in_block_comment = !in_block_comment;
                        }
                    } else {
                        // Line is exactly """ or ''' - toggle block comment state
                        in_block_comment = !in_block_comment;
                    }
                    continue;
                }
                if in_block_comment {
                    continue;
                }
                // Skip single-line comments
                if trimmed.starts_with('#') {
                    continue;
                }
            }
            _ => {}
        }

        count += 1;
    }

    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{SliceType, SlicesConfig, VsaConfig};
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn create_test_config(root: PathBuf) -> VsaConfig {
        VsaConfig {
            version: 2,
            architecture: crate::config::ArchitectureType::HexagonalEventSourcedVsa,
            root,
            language: "typescript".to_string(),
            domain: None,
            slices: Some(SlicesConfig {
                path: PathBuf::from("slices"),
                types: vec![SliceType::Command, SliceType::Query, SliceType::Saga],
                metadata_file: "slice.yaml".to_string(),
                command: None,
                query: None,
                saga: None,
            }),
            infrastructure: None,
            framework: None,
            contexts: HashMap::new(),
            validation: crate::config::ValidationConfig::default(),
            patterns: crate::config::PatternsConfig::default(),
        }
    }

    #[test]
    fn test_extract_typescript_imports() {
        let content = r#"
import { CreateOrderCommand } from '../domain/commands/CreateOrderCommand';
import { OrderRepository } from '../infrastructure/OrderRepository';
import { Handler } from '@event-sourcing/core';
const utils = require('../utils/helpers');
"#;

        let imports = extract_typescript_imports(content);
        assert_eq!(imports.len(), 4);
        assert!(imports.contains(&"../domain/commands/CreateOrderCommand".to_string()));
        assert!(imports.contains(&"../infrastructure/OrderRepository".to_string()));
        assert!(imports.contains(&"@event-sourcing/core".to_string()));
        assert!(imports.contains(&"../utils/helpers".to_string()));
    }

    #[test]
    fn test_extract_python_imports() {
        let content = r#"
from domain.commands import CreateOrderCommand
from infrastructure.repositories import OrderRepository
import os
from slices.other_slice.handler import OtherHandler
"#;

        let imports = extract_python_imports(content);
        assert!(imports.contains(&"domain.commands".to_string()));
        assert!(imports.contains(&"infrastructure.repositories".to_string()));
        assert!(imports.contains(&"slices.other_slice.handler".to_string()));
    }

    #[test]
    fn test_detect_cross_slice_imports() {
        let content = r#"
import { Something } from '../list_orders/OrderListProjection';
import { Handler } from '@event-sourcing/core';
"#;

        let all_slices = vec!["create_order", "list_orders", "get_order_detail"];
        let violations =
            detect_cross_slice_imports(content, "create_order", &all_slices, "typescript");

        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].imported_slice, "list_orders");
    }

    #[test]
    fn test_no_cross_slice_import_from_domain() {
        let content = r#"
import { CreateOrderCommand } from '../domain/commands/CreateOrderCommand';
import { OrderCreatedEvent } from '../domain/events/OrderCreatedEvent';
"#;

        let all_slices = vec!["create_order", "list_orders"];
        let violations =
            detect_cross_slice_imports(content, "create_order", &all_slices, "typescript");

        // Imports from domain should NOT be violations
        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_count_code_lines_typescript() {
        let content = r#"
// This is a comment
import { Handler } from '@event-sourcing/core';

/*
 * Block comment
 * Multiple lines
 */
export class CreateOrderController {
    // Another comment
    async handle(request: Request) {
        return response;
    }
}
"#;

        let count = count_code_lines(content, "typescript");
        // Should count: import, export class, async handle, return, closing braces
        assert_eq!(count, 6);
    }

    #[test]
    fn test_count_code_lines_python() {
        let content = r#"
# This is a comment
from domain import Command

"""
Docstring
Multiple lines
"""
class CreateOrderController:
    # Another comment
    def handle(self, request):
        return response
"#;

        let count = count_code_lines(content, "python");
        // Should count: from import, class, def, return
        assert_eq!(count, 4);
    }

    #[test]
    fn test_vsa010_cross_slice_import() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        // Create two slices
        let slice1 = root.join("slices/create_order");
        let slice2 = root.join("slices/list_orders");
        fs::create_dir_all(&slice1).unwrap();
        fs::create_dir_all(&slice2).unwrap();

        // Slice 1 imports from Slice 2 (violation!)
        fs::write(
            slice1.join("CreateOrderController.ts"),
            r#"
import { OrderListProjection } from '../list_orders/OrderListProjection';

export class CreateOrderController {}
"#,
        )
        .unwrap();
        fs::write(slice1.join("CreateOrderCommand.ts"), "export class CreateOrderCommand {}")
            .unwrap();

        fs::write(slice2.join("ListOrdersQuery.ts"), "export class ListOrdersQuery {}").unwrap();
        fs::write(slice2.join("OrderListProjection.ts"), "export class OrderListProjection {}")
            .unwrap();

        let config = create_test_config(root.clone());
        let ctx = ValidationContext::new(config, root);
        let mut report = EnhancedValidationReport::default();

        let rule = NoCrossSliceImportsRule;
        rule.validate(&ctx, &mut report).unwrap();

        assert_eq!(report.errors.len(), 1);
        assert_eq!(report.errors[0].code, "VSA010");
        assert!(report.errors[0].message.contains("Cross-slice import"));
    }

    #[test]
    fn test_vsa011_thin_adapter() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        // Create slice with fat controller (>50 lines)
        let slice_path = root.join("slices/create_order");
        fs::create_dir_all(&slice_path).unwrap();

        // Create a controller with 60 lines of actual code
        let mut content = String::from("export class CreateOrderController {\n");
        for i in 0..58 {
            content.push_str(&format!("    line{i}: string;\n"));
        }
        content.push_str("}\n");

        fs::write(slice_path.join("CreateOrderController.ts"), content).unwrap();
        fs::write(slice_path.join("CreateOrderCommand.ts"), "export class CreateOrderCommand {}")
            .unwrap();

        let config = create_test_config(root.clone());
        let ctx = ValidationContext::new(config, root);
        let mut report = EnhancedValidationReport::default();

        let rule = ThinAdapterRule;
        rule.validate(&ctx, &mut report).unwrap();

        assert_eq!(report.warnings.len(), 1);
        assert_eq!(report.warnings[0].code, "VSA011");
        assert!(report.warnings[0].message.contains("exceeding"));
    }

    #[test]
    fn test_vsa011_thin_adapter_ok() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        // Create slice with thin controller (<50 lines)
        let slice_path = root.join("slices/create_order");
        fs::create_dir_all(&slice_path).unwrap();

        fs::write(
            slice_path.join("CreateOrderController.ts"),
            r#"
import { CommandBus } from '../infrastructure/CommandBus';
import { CreateOrderCommand } from './CreateOrderCommand';

export class CreateOrderController {
    constructor(private commandBus: CommandBus) {}

    async handle(request: Request): Promise<Response> {
        const command = new CreateOrderCommand(request.body);
        await this.commandBus.execute(command);
        return new Response({ status: 'ok' });
    }
}
"#,
        )
        .unwrap();
        fs::write(slice_path.join("CreateOrderCommand.ts"), "export class CreateOrderCommand {}")
            .unwrap();

        let config = create_test_config(root.clone());
        let ctx = ValidationContext::new(config, root);
        let mut report = EnhancedValidationReport::default();

        let rule = ThinAdapterRule;
        rule.validate(&ctx, &mut report).unwrap();

        // Should have no warnings
        assert_eq!(report.warnings.len(), 0);
    }
}
