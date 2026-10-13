//! Slice types as Event Modeling patterns.
//!
//! A slice declares its pattern in `slice.yaml` (`type:`). These rules check
//! that the declared type is one VSA knows, that the project allows it, that
//! old names are flagged, and that automation and view slices have the shape
//! their pattern needs:
//!
//! - VSA036 (error): `type:` is not a known slice type.
//! - VSA037 (error): `type:` is known but not in `slices.types`.
//! - VSA038 (warning): `query` / `saga` are deprecated names for `view` /
//!   `automation`, in `slice.yaml` or in `slices.types`.
//! - VSA039 (error): an automation slice needs a to-do list view (a projection,
//!   or a file named `*todo*`) and must issue a command. A translation slice
//!   must issue a command.
//! - VSA040 (error): a view slice must not issue a command. A projection runner
//!   that only keeps a view current is a view; once it issues commands it is an
//!   automation and should say so.
//!
//! VSA039 and VSA040 are on by default for `hexagonal-event-sourced-vsa` and
//! off for other architectures (`slices.saga.require_todo_view`,
//! `slices.saga.require_command`, `slices.query.forbid_commands`).
//!
//! "Issues a command" is read from non-test source files, in one of two ways:
//! an import from a `commands` module (where VSA020 puts commands), or a type
//! name ending in `Command` (`PlaceOrderCommand`, `ItemCommand::Place`).
//! Comment lines are skipped. It is a name-based reading, so a command type
//! named without the suffix is not seen.

use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;

use super::import_parser::parse_imports;
use super::{
    is_test_or_conftest_name, EnhancedValidationReport, Severity, Suggestion, ValidationContext,
    ValidationIssue,
};
use crate::config::{ArchitectureType, SliceType, VsaConfig};
use crate::error::Result;
use crate::scanners::{Slice, SliceFileType, SliceScanner};

use super::rules::ValidationRule;

fn scan(ctx: &ValidationContext) -> Result<Vec<Slice>> {
    SliceScanner::for_config(&ctx.config, &ctx.root).scan()
}

/// The `type:` the slice's manifest wrote, if any.
fn written_type(slice: &Slice) -> Option<&str> {
    slice.metadata.as_ref().map(|m| m.slice_type.as_str()).filter(|t| !t.is_empty())
}

fn valid_names() -> String {
    "command, view, automation, translation (deprecated: query, saga; legacy: mixed)".to_string()
}

/// VSA036: a declared slice type must be known.
pub struct UnknownSliceTypeRule;

impl ValidationRule for UnknownSliceTypeRule {
    fn name(&self) -> &str {
        "unknown-slice-type"
    }

    fn code(&self) -> &str {
        "VSA036"
    }

    fn validate(
        &self,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        for slice in scan(ctx)? {
            let Some(written) = written_type(&slice) else { continue };
            if SliceType::from_name(written).is_some() {
                continue;
            }
            report.errors.push(ValidationIssue {
                path: slice.path.clone(),
                code: self.code().to_string(),
                severity: Severity::Error,
                message: format!(
                    "Slice '{}' declares unknown type '{}'. Known types: {}.",
                    slice.name,
                    written,
                    valid_names()
                ),
                suggestions: vec![Suggestion::manual(
                    "Set type: in slice.yaml to the Event Modeling pattern the slice implements",
                )],
            });
        }
        Ok(())
    }
}

/// VSA037: a declared slice type must be one the project allows.
pub struct UndeclaredSliceTypeRule;

impl ValidationRule for UndeclaredSliceTypeRule {
    fn name(&self) -> &str {
        "slice-type-not-allowed"
    }

    fn code(&self) -> &str {
        "VSA037"
    }

    fn validate(
        &self,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        let Some(slices_config) = ctx.config.slices.as_ref() else {
            return Ok(());
        };
        let allowed: Vec<SliceType> = slices_config.types.iter().map(SliceType::pattern).collect();
        for slice in scan(ctx)? {
            let Some(declared) = written_type(&slice).and_then(SliceType::from_name) else {
                continue;
            };
            if allowed.contains(&declared.pattern()) {
                continue;
            }
            let allowed_names: Vec<&str> =
                slices_config.types.iter().map(SliceType::name).collect();
            report.errors.push(ValidationIssue {
                path: slice.path.clone(),
                code: self.code().to_string(),
                severity: Severity::Error,
                message: format!(
                    "Slice '{}' is type '{}', which slices.types [{}] does not allow.",
                    slice.name,
                    declared.name(),
                    allowed_names.join(", ")
                ),
                suggestions: vec![Suggestion::manual(format!(
                    "Add {} to slices.types in vsa.yaml, or change the slice's type",
                    declared.pattern().name()
                ))],
            });
        }
        Ok(())
    }
}

/// VSA038: `query` and `saga` are deprecated names.
pub struct DeprecatedSliceTypeNameRule;

impl ValidationRule for DeprecatedSliceTypeNameRule {
    fn name(&self) -> &str {
        "deprecated-slice-type-name"
    }

    fn code(&self) -> &str {
        "VSA038"
    }

    fn validate(
        &self,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        let mut warn = |path: &Path, message: String, new: &str| {
            report.warnings.push(ValidationIssue {
                path: path.to_path_buf(),
                code: self.code().to_string(),
                severity: Severity::Warning,
                message,
                suggestions: vec![Suggestion::manual(format!("Use {new}"))],
            });
        };

        if let Some(slices_config) = ctx.config.slices.as_ref() {
            for slice_type in &slices_config.types {
                if let Some((old, new)) = slice_type.deprecated_name() {
                    warn(
                        &ctx.root,
                        format!(
                            "slices.types lists '{old}', the deprecated name for '{new}' \
                             (Event Modeling pattern names)."
                        ),
                        new,
                    );
                }
            }
        }

        for slice in scan(ctx)? {
            let Some(declared) = written_type(&slice).and_then(SliceType::from_name) else {
                continue;
            };
            if let Some((old, new)) = declared.deprecated_name() {
                warn(
                    &slice.path,
                    format!(
                        "Slice '{}' uses type '{old}', the deprecated name for '{new}'.",
                        slice.name
                    ),
                    new,
                );
            }
        }
        Ok(())
    }
}

/// Which pattern-shape checks apply, after defaults and config overrides.
struct ShapePolicy {
    automation_needs_todo_view: bool,
    automation_needs_command: bool,
    view_forbids_commands: bool,
}

impl ShapePolicy {
    fn from_config(config: &VsaConfig) -> Self {
        let on = config.architecture == ArchitectureType::HexagonalEventSourcedVsa;
        let slices = config.slices.as_ref();
        let saga = slices.and_then(|s| s.saga.as_ref());
        let query = slices.and_then(|s| s.query.as_ref());
        Self {
            automation_needs_todo_view: saga.and_then(|s| s.require_todo_view).unwrap_or(on),
            automation_needs_command: saga.and_then(|s| s.require_command).unwrap_or(on),
            view_forbids_commands: query.and_then(|q| q.forbid_commands).unwrap_or(on),
        }
    }
}

fn command_type_name() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b[A-Z][A-Za-z0-9_]*Command\b").expect("valid regex"))
}

fn is_comment(line: &str) -> bool {
    let line = line.trim_start();
    line.starts_with("//")
        || line.starts_with('#')
        || line.starts_with('*')
        || line.starts_with("/*")
}

/// Where in the slice's non-test source a command is issued, if anywhere.
fn first_command_reference(slice: &Slice) -> Option<(String, String)> {
    for file in &slice.files {
        if matches!(file.file_type, SliceFileType::Test | SliceFileType::Manifest)
            || is_test_or_conftest_name(&file.name)
            || !is_source(&file.name)
        {
            continue;
        }
        for import in parse_imports(&file.path).unwrap_or_default() {
            if import.module.split([':', '/', '.']).any(|s| s == "commands") {
                return Some((file.name.clone(), import.module));
            }
        }
        let Ok(source) = std::fs::read_to_string(&file.path) else { continue };
        for line in source.lines().filter(|l| !is_comment(l)) {
            if let Some(m) = command_type_name().find(line) {
                return Some((file.name.clone(), m.as_str().to_string()));
            }
        }
    }
    None
}

fn is_source(name: &str) -> bool {
    [".rs", ".py", ".ts", ".tsx"].iter().any(|ext| name.ends_with(ext))
}

fn has_todo_view(slice: &Slice) -> bool {
    slice.has_projection()
        || slice.files.iter().any(|f| {
            let lower = f.name.to_lowercase();
            f.file_type != SliceFileType::Test
                && (lower.contains("todo") || lower.contains("to_do"))
        })
}

/// VSA039: automation and translation slices issue a command; automation keeps
/// a to-do list view.
pub struct AutomationSliceShapeRule;

impl ValidationRule for AutomationSliceShapeRule {
    fn name(&self) -> &str {
        "automation-slice-shape"
    }

    fn code(&self) -> &str {
        "VSA039"
    }

    fn validate(
        &self,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        let policy = ShapePolicy::from_config(&ctx.config);
        for slice in scan(ctx)? {
            let pattern = slice.slice_type.pattern();
            if !matches!(pattern, SliceType::Automation | SliceType::Translation) {
                continue;
            }
            let mut missing = Vec::new();
            if pattern == SliceType::Automation
                && policy.automation_needs_todo_view
                && !has_todo_view(&slice)
            {
                missing.push("a to-do list view of pending work (a projection, or a *todo* file)");
            }
            if policy.automation_needs_command && first_command_reference(&slice).is_none() {
                missing.push("a command it issues (an import from commands/ or a *Command type)");
            }
            if missing.is_empty() {
                continue;
            }
            report.errors.push(ValidationIssue {
                path: slice.path.clone(),
                code: self.code().to_string(),
                severity: Severity::Error,
                message: format!(
                    "{} slice '{}' is missing {}. An automation reads a to-do list view, takes \
                     the next entry and issues a command; the event that command produces clears \
                     the entry. It is not event-in/event-out.",
                    pattern.name(),
                    slice.name,
                    missing.join(" and ")
                ),
                suggestions: vec![Suggestion::manual(
                    "If the slice only keeps a read model current, it is a view: set type: view",
                )],
            });
        }
        Ok(())
    }
}

/// VSA040: a view slice issues no command.
pub struct ViewSliceIssuesNoCommandRule;

impl ValidationRule for ViewSliceIssuesNoCommandRule {
    fn name(&self) -> &str {
        "view-slice-issues-no-command"
    }

    fn code(&self) -> &str {
        "VSA040"
    }

    fn validate(
        &self,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        if !ShapePolicy::from_config(&ctx.config).view_forbids_commands {
            return Ok(());
        }
        for slice in scan(ctx)? {
            if slice.slice_type.pattern() != SliceType::View {
                continue;
            }
            let Some((file, reference)) = first_command_reference(&slice) else { continue };
            report.errors.push(ValidationIssue {
                path: slice.path.clone(),
                code: self.code().to_string(),
                severity: Severity::Error,
                message: format!(
                    "View slice '{}' issues a command ('{}' in {}). A view only keeps a read model \
                     current from events. A slice that turns pending work into commands is an \
                     automation: declare type: automation.",
                    slice.name, reference, file
                ),
                suggestions: vec![Suggestion::manual(
                    "Set type: automation, or remove the command",
                )],
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        DomainConfig, PatternsConfig, QuerySliceConfig, SagaSliceConfig, SlicesConfig,
        ValidationConfig,
    };
    use std::collections::HashMap;
    use std::fs;
    use tempfile::TempDir;

    fn make_ctx(
        architecture: ArchitectureType,
        slices: SlicesConfig,
    ) -> (TempDir, ValidationContext) {
        let temp = TempDir::new().unwrap();
        let root = temp.path().to_path_buf();
        let config = VsaConfig {
            version: 2,
            architecture,
            root: root.clone(),
            language: "rust".to_string(),
            domain: Some(DomainConfig::default()),
            slices: Some(slices),
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

    fn hex() -> (TempDir, ValidationContext) {
        make_ctx(ArchitectureType::HexagonalEventSourcedVsa, SlicesConfig::default())
    }

    fn slice(root: &Path, name: &str, slice_type: &str, files: &[(&str, &str)]) {
        slice_in(&root.join("orders/slices"), name, slice_type, files);
    }

    fn slice_in(slices: &Path, name: &str, slice_type: &str, files: &[(&str, &str)]) {
        let dir = slices.join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("slice.yaml"), format!("name: {name}\ntype: {slice_type}\n")).unwrap();
        for (file, body) in files {
            let path = dir.join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, body).unwrap();
        }
    }

    fn codes(rule: &dyn ValidationRule, ctx: &ValidationContext) -> (Vec<String>, Vec<String>) {
        let mut report = EnhancedValidationReport::default();
        rule.validate(ctx, &mut report).unwrap();
        (
            report.errors.iter().map(|e| e.message.clone()).collect(),
            report.warnings.iter().map(|w| w.message.clone()).collect(),
        )
    }

    const TODO_PROJECTION: &str = "pub struct PendingPlacements; // keeps the to-do list\n";
    const PROCESSOR: &str = "use crate::domain::{decide, ItemCommand};\n\
                             pub fn next() { let _ = decide(ItemCommand::Place); }\n";

    #[test]
    fn unknown_type_is_an_error() {
        let (_t, ctx) = hex();
        slice(&ctx.root, "place", "bogus", &[]);
        let (errors, _) = codes(&UnknownSliceTypeRule, &ctx);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("unknown type 'bogus'"));
    }

    #[test]
    fn type_outside_slices_types_is_an_error() {
        let slices = SlicesConfig {
            types: vec![SliceType::Command, SliceType::Query],
            ..SlicesConfig::default()
        };
        let (_t, ctx) = make_ctx(ArchitectureType::HexagonalEventSourcedVsa, slices);
        slice(&ctx.root, "place", "saga", &[]);
        slice(&ctx.root, "list", "view", &[]); // view == query: allowed
        let (errors, _) = codes(&UndeclaredSliceTypeRule, &ctx);
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].contains("'saga'"));
    }

    #[test]
    fn old_names_warn_in_manifest_and_config() {
        let slices = SlicesConfig {
            types: vec![SliceType::Command, SliceType::Query],
            ..SlicesConfig::default()
        };
        let (_t, ctx) = make_ctx(ArchitectureType::HexagonalEventSourcedVsa, slices);
        slice(&ctx.root, "process", "saga", &[]);
        slice(&ctx.root, "list", "view", &[]);
        let (errors, warnings) = codes(&DeprecatedSliceTypeNameRule, &ctx);
        assert!(errors.is_empty());
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(warnings.iter().any(|w| w.contains("slices.types lists 'query'")));
        assert!(warnings
            .iter()
            .any(|w| w.contains("'saga', the deprecated name for 'automation'")));
    }

    #[test]
    fn automation_with_todo_view_and_command_passes() {
        let (_t, ctx) = hex();
        slice(
            &ctx.root,
            "place_pending",
            "automation",
            &[("pending_placements_projection.rs", TODO_PROJECTION), ("processor.rs", PROCESSOR)],
        );
        assert!(codes(&AutomationSliceShapeRule, &ctx).0.is_empty());
    }

    #[test]
    fn automation_without_view_or_command_fails() {
        let (_t, ctx) = hex();
        // Event in, event out: no to-do list, no command.
        slice(
            &ctx.root,
            "relay",
            "automation",
            &[("relay.rs", "pub fn on(e: Placed) -> Moved { todo!() }\n")],
        );
        let (errors, _) = codes(&AutomationSliceShapeRule, &ctx);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("to-do list view"));
        assert!(errors[0].contains("a command it issues"));
    }

    #[test]
    fn saga_is_checked_as_automation() {
        let (_t, ctx) = hex();
        slice(&ctx.root, "relay", "saga", &[("relay_projection.rs", TODO_PROJECTION)]);
        let (errors, _) = codes(&AutomationSliceShapeRule, &ctx);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].starts_with("automation slice 'relay'"));
    }

    #[test]
    fn translation_must_issue_a_command_but_needs_no_todo_view() {
        let (_t, ctx) = hex();
        slice(&ctx.root, "import_github", "translation", &[("webhook.rs", PROCESSOR)]);
        slice(&ctx.root, "import_jira", "translation", &[("webhook.rs", "pub fn on() {}\n")]);
        let (errors, _) = codes(&AutomationSliceShapeRule, &ctx);
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].contains("'import_jira'"));
    }

    #[test]
    fn projection_runner_that_issues_a_command_fails_as_a_view() {
        let (_t, ctx) = hex();
        slice(&ctx.root, "keeps_current", "view", &[("totals_projection.rs", TODO_PROJECTION)]);
        slice(&ctx.root, "dispatches", "query", &[("runner_projection.rs", PROCESSOR)]);
        let (errors, _) = codes(&ViewSliceIssuesNoCommandRule, &ctx);
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(
            errors[0]
                .contains("'dispatches' issues a command ('ItemCommand' in runner_projection.rs)"),
            "{}",
            errors[0]
        );
    }

    #[test]
    fn command_named_only_in_a_comment_or_test_does_not_count() {
        let (_t, ctx) = hex();
        slice(
            &ctx.root,
            "totals",
            "view",
            &[
                (
                    "totals_projection.rs",
                    "// folds PlaceCommand outcomes into totals\npub struct T;\n",
                ),
                ("totals_projection_test.rs", "use crate::domain::commands::PlaceCommand;\n"),
            ],
        );
        assert!(codes(&ViewSliceIssuesNoCommandRule, &ctx).0.is_empty());
    }

    #[test]
    fn shape_checks_are_off_by_default_elsewhere_and_configurable() {
        let (_t, ctx) = make_ctx(ArchitectureType::default(), SlicesConfig::default());
        // Single-root architectures scan <root>/slices.
        let slices_dir = ctx.root.join("slices");
        slice_in(&slices_dir, "relay", "automation", &[("relay.rs", "pub fn on() {}\n")]);
        slice_in(&slices_dir, "dispatches", "view", &[("runner_projection.rs", PROCESSOR)]);
        assert_eq!(scan(&ctx).unwrap().len(), 2, "the slices are seen");
        assert!(codes(&AutomationSliceShapeRule, &ctx).0.is_empty());
        assert!(codes(&ViewSliceIssuesNoCommandRule, &ctx).0.is_empty());

        let slices = SlicesConfig {
            query: Some(QuerySliceConfig {
                forbid_commands: Some(false),
                ..QuerySliceConfig::default()
            }),
            saga: Some(SagaSliceConfig {
                require_todo_view: Some(false),
                require_command: Some(false),
                ..SagaSliceConfig::default()
            }),
            ..SlicesConfig::default()
        };
        let (_t, ctx) = make_ctx(ArchitectureType::HexagonalEventSourcedVsa, slices);
        slice(&ctx.root, "relay", "automation", &[("relay.rs", "pub fn on() {}\n")]);
        slice(&ctx.root, "dispatches", "view", &[("runner_projection.rs", PROCESSOR)]);
        assert!(codes(&AutomationSliceShapeRule, &ctx).0.is_empty());
        assert!(codes(&ViewSliceIssuesNoCommandRule, &ctx).0.is_empty());
    }
}
