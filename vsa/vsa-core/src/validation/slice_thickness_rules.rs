//! Slice thickness: a command slice must do a command slice's job.
//!
//! The dependency rules say what a slice may NOT import. These rules say what a
//! command slice MUST reach, so a slice that decides on its own and writes
//! nowhere cannot pass as one:
//!
//! - VSA034: a command slice imports the domain layer. The decision (an
//!   aggregate's `decide`, or a command handler on the aggregate) lives there.
//! - VSA035: a command slice imports a port. That is how it reaches storage
//!   (the event store) without depending on an adapter.
//!
//! Both are on by default for `hexagonal-event-sourced-vsa` and off for other
//! architectures. Turn either off with `slices.command.require_domain_call` or
//! `slices.command.require_port`.
//!
//! What these rules do NOT check: whether business invariants live in a
//! handler. A handler that imports the domain and still validates on its own
//! passes. Telling a domain invariant from input parsing or port error mapping
//! needs a reading of the code, not a pattern. A guard-clause count would flag
//! thin handlers that map errors and miss fat ones written as `match` arms.
//! The defensible proxies are structural: these two rules, the line budget on
//! adapters (VSA011) and slice isolation (VSA010/VSA031).

use std::collections::HashSet;

use super::source_refs::referenced_segments;
use super::{
    is_test_or_conftest_name, EnhancedValidationReport, Severity, Suggestion, ValidationContext,
    ValidationIssue,
};
use crate::config::{ArchitectureType, SliceType, VsaConfig};
use crate::error::Result;
use crate::scanners::{Slice, SliceFileType, SliceScanner};

use super::rules::ValidationRule;

/// Which thickness checks apply, after defaults and config overrides.
struct ThicknessPolicy {
    require_domain: bool,
    require_port: bool,
    domain_segment: String,
}

impl ThicknessPolicy {
    fn from_config(config: &VsaConfig) -> Self {
        let on_by_default = config.architecture == ArchitectureType::HexagonalEventSourcedVsa;
        let command = config.slices.as_ref().and_then(|s| s.command.as_ref());
        let domain_segment = config
            .domain
            .as_ref()
            .and_then(|d| d.path.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("domain")
            .to_string();
        Self {
            require_domain: command.and_then(|c| c.require_domain_call).unwrap_or(on_by_default),
            require_port: command.and_then(|c| c.require_port).unwrap_or(on_by_default),
            domain_segment,
        }
    }
}

/// Layer names and identifiers the slice's non-test source files refer to
/// (imports, and Rust qualified paths), with comments, literals and
/// test-only code left out.
fn imported_segments(slice: &Slice) -> HashSet<String> {
    let mut segments = HashSet::new();
    for file in &slice.files {
        if matches!(file.file_type, SliceFileType::Test | SliceFileType::Manifest)
            || is_test_or_conftest_name(&file.name)
        {
            continue;
        }
        segments.extend(referenced_segments(&file.path));
    }
    segments
}

fn command_slices(ctx: &ValidationContext) -> Result<Vec<Slice>> {
    let scanner = SliceScanner::for_config(&ctx.config, &ctx.root);
    Ok(scanner.scan()?.into_iter().filter(|s| s.has_pattern(&SliceType::Command)).collect())
}

/// VSA034: a command slice must reach the domain.
pub struct CommandSliceReachesDomainRule;

impl ValidationRule for CommandSliceReachesDomainRule {
    fn name(&self) -> &str {
        "command-slice-reaches-domain"
    }

    fn code(&self) -> &str {
        "VSA034"
    }

    fn validate(
        &self,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        let policy = ThicknessPolicy::from_config(&ctx.config);
        if !policy.require_domain {
            return Ok(());
        }
        for slice in command_slices(ctx)? {
            if imported_segments(&slice).contains(&policy.domain_segment) {
                continue;
            }
            report.errors.push(ValidationIssue {
                path: slice.path.clone(),
                code: self.code().to_string(),
                severity: Severity::Error,
                message: format!(
                    "Command slice '{}' never imports the {} layer. A command slice hands the \
                     decision to the domain (an aggregate's decide, or a command handler on the \
                     aggregate); a slice that does not reach the domain is deciding on its own.",
                    slice.name, policy.domain_segment
                ),
                suggestions: vec![
                    Suggestion::manual(
                        "Call the aggregate's decide (or command handler) and append what it returns",
                    ),
                    Suggestion::manual(
                        "Opt out with slices.command.require_domain_call: false",
                    ),
                ],
            });
        }
        Ok(())
    }
}

/// VSA035: a command slice must reach a port.
pub struct CommandSliceReachesPortRule;

impl ValidationRule for CommandSliceReachesPortRule {
    fn name(&self) -> &str {
        "command-slice-reaches-port"
    }

    fn code(&self) -> &str {
        "VSA035"
    }

    fn validate(
        &self,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        let policy = ThicknessPolicy::from_config(&ctx.config);
        if !policy.require_port {
            return Ok(());
        }
        for slice in command_slices(ctx)? {
            if imported_segments(&slice).contains("ports") {
                continue;
            }
            report.errors.push(ValidationIssue {
                path: slice.path.clone(),
                code: self.code().to_string(),
                severity: Severity::Error,
                message: format!(
                    "Command slice '{}' never imports a port (ports/). A command slice persists \
                     what the domain decided through a port such as the event store; a slice \
                     that reaches no port changes nothing.",
                    slice.name
                ),
                suggestions: vec![
                    Suggestion::manual("Append the decided events through the event store port"),
                    Suggestion::manual("Opt out with slices.command.require_port: false"),
                ],
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        CommandSliceConfig, DomainConfig, PatternsConfig, SlicesConfig, ValidationConfig,
    };
    use std::collections::HashMap;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    fn ctx(
        architecture: ArchitectureType,
        command: Option<CommandSliceConfig>,
    ) -> (TempDir, ValidationContext) {
        let temp = TempDir::new().unwrap();
        let root = temp.path().to_path_buf();
        let config = VsaConfig {
            version: 2,
            architecture,
            root: root.clone(),
            language: "rust".to_string(),
            domain: Some(DomainConfig::default()),
            slices: Some(SlicesConfig { command, ..SlicesConfig::default() }),
            infrastructure: None,
            framework: None,
            contexts: HashMap::new(),
            validation: ValidationConfig::default(),
            patterns: PatternsConfig::default(),
            projection_allowed_prefixes: None,
            cross_context_scan_paths: Vec::new(),
            exceptions: Vec::new(),
            layer_separation: None,
        };
        (temp, ValidationContext::new(config, root))
    }

    fn command_slice(root: &Path, name: &str, handler: &str) {
        command_slice_in(&root.join("orders/slices"), name, handler);
    }

    fn command_slice_in(slices: &Path, name: &str, handler: &str) {
        let dir = slices.join(name).join("internal");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.parent().unwrap().join("slice.yaml"),
            format!("name: {name}\ntype: command\n"),
        )
        .unwrap();
        fs::write(dir.join(format!("{name}_handler.rs")), handler).unwrap();
        // A test that imports both layers must not count.
        fs::write(
            dir.join(format!("{name}_handler_test.rs")),
            "use crate::domain::x;\nuse crate::ports::y;\n",
        )
        .unwrap();
    }

    const THIN: &str = "use crate::domain::{decide, OrderCommand};\nuse crate::ports::EventStorePort;\n\
                        pub fn place(s: &mut dyn EventStorePort) { s.append(decide(OrderCommand::Place)); }\n";
    const FAT: &str =
        "pub fn rename(title: &str, existing: &[String]) -> Result<String, String> {\n\
                       if title.is_empty() { return Err(\"empty\".into()); }\n\
                       if existing.iter().any(|t| t == title) { return Err(\"dup\".into()); }\n\
                       Ok(title.to_string())\n}\n";

    fn run_messages(rule: &dyn ValidationRule, ctx: &ValidationContext) -> Vec<String> {
        let mut report = EnhancedValidationReport::default();
        rule.validate(ctx, &mut report).unwrap();
        report.errors.iter().map(|e| e.message.clone()).collect()
    }

    fn run(rule: &dyn ValidationRule, ctx: &ValidationContext) -> Vec<String> {
        let mut report = EnhancedValidationReport::default();
        rule.validate(ctx, &mut report).unwrap();
        report.errors.iter().map(|e| e.code.clone()).collect()
    }

    #[test]
    fn thin_command_slice_passes_both() {
        let (_t, ctx) = ctx(ArchitectureType::HexagonalEventSourcedVsa, None);
        command_slice(&ctx.root, "place_order", THIN);
        assert!(run(&CommandSliceReachesDomainRule, &ctx).is_empty());
        assert!(run(&CommandSliceReachesPortRule, &ctx).is_empty());
    }

    #[test]
    fn fat_command_slice_fails_both() {
        let (_t, ctx) = ctx(ArchitectureType::HexagonalEventSourcedVsa, None);
        command_slice(&ctx.root, "rename", FAT);
        assert_eq!(run(&CommandSliceReachesDomainRule, &ctx), vec!["VSA034"]);
        assert_eq!(run(&CommandSliceReachesPortRule, &ctx), vec!["VSA035"]);
    }

    #[test]
    fn inline_rust_paths_count_as_reaching_a_layer() {
        let (_t, ctx) = ctx(ArchitectureType::HexagonalEventSourcedVsa, None);
        command_slice(
            &ctx.root,
            "place_order",
            "pub fn place(s: &mut dyn crate::ports::EventStorePort) { s.append(crate::domain::decide()); }\n",
        );
        assert!(run(&CommandSliceReachesDomainRule, &ctx).is_empty());
        assert!(run(&CommandSliceReachesPortRule, &ctx).is_empty());
    }

    #[test]
    fn grouped_imports_count_and_strings_comments_tests_do_not() {
        let (_t, ctx) = ctx(ArchitectureType::HexagonalEventSourcedVsa, None);
        command_slice(
            &ctx.root,
            "grouped",
            "use crate::{domain::decide, ports::EventStorePort};\npub fn f() {}\n",
        );
        command_slice(
            &ctx.root,
            "pretend",
            "// use crate::domain::decide;\n\
             pub fn f() { let _ = \"crate::domain::decide crate::ports::EventStorePort\"; }\n\
             #[cfg(test)]\nmod tests {\n    use crate::domain::decide;\n    use crate::ports::EventStorePort;\n}\n",
        );
        let domain = run_messages(&CommandSliceReachesDomainRule, &ctx);
        let port = run_messages(&CommandSliceReachesPortRule, &ctx);
        assert_eq!(domain.len(), 1, "{domain:?}");
        assert!(domain[0].contains("'pretend'"));
        assert_eq!(port.len(), 1, "{port:?}");
        assert!(port[0].contains("'pretend'"));
    }

    #[test]
    fn off_by_default_for_other_architectures() {
        let (_t, ctx) = ctx(ArchitectureType::default(), None);
        // Single-root architectures scan <root>/slices.
        command_slice_in(&ctx.root.join("slices"), "rename", FAT);
        assert_eq!(command_slices(&ctx).unwrap().len(), 1, "the slice is seen");
        assert!(run(&CommandSliceReachesDomainRule, &ctx).is_empty());
        assert!(run(&CommandSliceReachesPortRule, &ctx).is_empty());
    }

    #[test]
    fn config_can_turn_checks_on_elsewhere_and_independently() {
        let only_port = CommandSliceConfig {
            require_domain_call: Some(false),
            require_port: Some(true),
            ..CommandSliceConfig::default()
        };
        let (_t, ctx) = ctx(ArchitectureType::default(), Some(only_port));
        command_slice_in(&ctx.root.join("slices"), "rename", FAT);
        assert!(run(&CommandSliceReachesDomainRule, &ctx).is_empty());
        assert_eq!(run(&CommandSliceReachesPortRule, &ctx), vec!["VSA035"]);
    }

    #[test]
    fn config_can_turn_each_check_off() {
        let command = CommandSliceConfig {
            require_domain_call: Some(false),
            require_port: Some(false),
            ..CommandSliceConfig::default()
        };
        let (_t, ctx) = ctx(ArchitectureType::HexagonalEventSourcedVsa, Some(command));
        command_slice(&ctx.root, "rename", FAT);
        assert!(run(&CommandSliceReachesDomainRule, &ctx).is_empty());
        assert!(run(&CommandSliceReachesPortRule, &ctx).is_empty());
    }
}
