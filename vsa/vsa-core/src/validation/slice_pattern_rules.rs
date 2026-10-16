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
//! A slice may list several types (`type: [command, automation]`): each name
//! is checked by VSA036-VSA038 and each pattern's shape rule runs.
//!
//! VSA039 and VSA040 are on by default for `hexagonal-event-sourced-vsa` and
//! off for other architectures (`slices.saga.require_todo_view`,
//! `slices.saga.require_command`, `slices.query.forbid_commands`).
//!
//! "Issues a command" is read from non-test source files, in one of two ways:
//! an import from a `commands` module (where VSA020 puts commands), or a type
//! name ending in `Command` (`PlaceOrderCommand`, `ItemCommand::Place`).
//! Comments, string literals and Rust `#[cfg(test)]` code are not read. It
//! is a name-based reading, so a command type named without the suffix is not
//! seen.

use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;

use super::source_refs::{code_only, referenced_segments};
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

/// Every type the slice's manifest wrote that names a known slice type.
fn declared_types(slice: &Slice) -> Vec<SliceType> {
    slice.written_types().into_iter().filter_map(SliceType::from_name).collect()
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
            let written = slice.written_types();
            for name in &written {
                if SliceType::from_name(name).is_some() {
                    continue;
                }
                report.errors.push(ValidationIssue {
                    path: slice.path.clone(),
                    code: self.code().to_string(),
                    severity: Severity::Error,
                    message: format!(
                        "Slice '{}' declares unknown type '{}'. Known types: {}.",
                        slice.name,
                        name,
                        valid_names()
                    ),
                    suggestions: vec![Suggestion::manual(
                        "Set type: in slice.yaml to the Event Modeling pattern the slice implements",
                    )],
                });
            }
            // `mixed` names no pattern, so it cannot sit in a list of patterns.
            if written.len() > 1 && declared_types(&slice).contains(&SliceType::Mixed) {
                report.errors.push(ValidationIssue {
                    path: slice.path.clone(),
                    code: self.code().to_string(),
                    severity: Severity::Error,
                    message: format!(
                        "Slice '{}' lists 'mixed' among several types. A type list names each \
                         pattern the slice implements; 'mixed' names none.",
                        slice.name
                    ),
                    suggestions: vec![Suggestion::manual(
                        "List the patterns instead, e.g. type: [command, automation]",
                    )],
                });
            }
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
            for declared in declared_types(&slice) {
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
            for declared in declared_types(&slice) {
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

/// Where in the slice's non-test source a command is issued, if anywhere.
/// Comments, string literals and (Rust) `#[cfg(test)]` code are not read.
fn first_command_reference(slice: &Slice) -> Option<(String, String)> {
    for file in &slice.files {
        if matches!(file.file_type, SliceFileType::Test | SliceFileType::Manifest)
            || is_test_or_conftest_name(&file.name)
            || !is_source(&file.name)
        {
            continue;
        }
        if referenced_segments(&file.path).contains("commands") {
            return Some((file.name.clone(), "commands".to_string()));
        }
        if let Some(m) = command_type_name().find(&code_only(&file.path)) {
            return Some((file.name.clone(), m.as_str().to_string()));
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
            // An automation also listed as a translation is checked once, as
            // the stricter automation.
            let pattern = if slice.has_pattern(&SliceType::Automation) {
                SliceType::Automation
            } else if slice.has_pattern(&SliceType::Translation) {
                SliceType::Translation
            } else {
                continue;
            };
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
            let (why, suggestion) = if pattern == SliceType::Translation {
                (
                    "A translation takes what another system reports and issues a command in \
                     this system's language; without a command it changes nothing here.",
                    "Issue the command the external fact translates to",
                )
            } else {
                (
                    "An automation reads a to-do list view, takes the next entry and issues a \
                     command; the event that command produces clears the entry. It is not \
                     event-in/event-out.",
                    "If the slice only keeps a read model current, it is a view: set type: view",
                )
            };
            report.errors.push(ValidationIssue {
                path: slice.path.clone(),
                code: self.code().to_string(),
                severity: Severity::Error,
                message: format!(
                    "{} slice '{}' is missing {}. {why}",
                    pattern.name(),
                    slice.name,
                    missing.join(" and ")
                ),
                suggestions: vec![Suggestion::manual(suggestion)],
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
            if !slice.has_pattern(&SliceType::View) {
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
                    "// folds PlaceCommand outcomes into totals\npub const LABEL: &str = \"PlaceCommand\";\npub struct T;\n#[cfg(test)]\nmod t { use crate::domain::commands::PlaceCommand; }\n",
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

    #[test]
    fn empty_or_non_string_type_is_unknown() {
        let (_t, ctx) = hex();
        for (name, value) in [
            ("empty", "\"\""),
            ("number", "123"),
            ("empty_list", "[]"),
            ("list_with_number", "[command, 123]"),
            ("list_with_empty", "[command, \"\"]"),
        ] {
            let dir = ctx.root.join("orders/slices").join(name);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("slice.yaml"), format!("name: {name}\ntype: {value}\n")).unwrap();
        }
        let (errors, _) = codes(&UnknownSliceTypeRule, &ctx);
        assert_eq!(errors.len(), 5, "{errors:?}");
    }

    // A command slice that also keeps the to-do list and the processor issuing
    // its command: decides through the domain, appends through a port.
    const HANDLER: &str = "use crate::domain::{decide, ItemCommand};\n\
                           use crate::ports::EventStore;\n\
                           pub fn handle(s: &mut impl EventStore) { let _ = decide(ItemCommand::Place); }\n";

    fn all_errors(ctx: &ValidationContext) -> Vec<String> {
        use crate::validation::{CommandSliceReachesDomainRule, CommandSliceReachesPortRule};
        let rules: [&dyn ValidationRule; 6] = [
            &UnknownSliceTypeRule,
            &UndeclaredSliceTypeRule,
            &AutomationSliceShapeRule,
            &ViewSliceIssuesNoCommandRule,
            &CommandSliceReachesDomainRule,
            &CommandSliceReachesPortRule,
        ];
        rules.iter().flat_map(|r| codes(*r, ctx).0).collect()
    }

    #[test]
    fn a_one_entry_list_is_that_type() {
        let (_t, ctx) = hex();
        slice(&ctx.root, "place", "[command]", &[("place_handler.rs", HANDLER)]);
        let slices = scan(&ctx).unwrap();
        assert_eq!(slices[0].slice_type, SliceType::Command);
        assert_eq!(slices[0].patterns(), vec![SliceType::Command]);
        assert!(all_errors(&ctx).is_empty());
    }

    #[test]
    fn command_hosting_an_automation_passes_both_sets_of_checks() {
        let (_t, ctx) = hex();
        slice(
            &ctx.root,
            "place",
            "[command, automation]",
            &[
                ("place_handler.rs", HANDLER),
                ("place_to_do.rs", "pub struct PendingPlacements;\n"),
                ("place_process.rs", PROCESSOR),
            ],
        );
        let slices = scan(&ctx).unwrap();
        assert_eq!(slices[0].slice_type, SliceType::Command, "the first entry stays primary");
        assert_eq!(slices[0].patterns(), vec![SliceType::Command, SliceType::Automation]);
        assert!(all_errors(&ctx).is_empty(), "{:?}", all_errors(&ctx));
    }

    #[test]
    fn command_hosting_an_automation_runs_the_automation_checks() {
        let (_t, ctx) = hex();
        // Reaches the domain and a port, but keeps no to-do list.
        slice(&ctx.root, "place", "[command, automation]", &[("place_handler.rs", HANDLER)]);
        let errors = all_errors(&ctx);
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].contains("automation slice 'place' is missing a to-do list view"));
    }

    #[test]
    fn command_hosting_an_automation_runs_the_command_checks() {
        let (_t, ctx) = hex();
        // A to-do list and a command, but the slice never reaches a port.
        slice(
            &ctx.root,
            "place",
            "[automation, command]",
            &[
                ("place_to_do.rs", "pub struct PendingPlacements;\n"),
                ("place_process.rs", PROCESSOR),
            ],
        );
        let errors = all_errors(&ctx);
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].contains("Command slice 'place' never imports a port"));
    }

    #[test]
    fn each_listed_type_must_be_known_allowed_and_current() {
        let slices = SlicesConfig {
            types: vec![SliceType::Command, SliceType::View],
            ..SlicesConfig::default()
        };
        let (_t, ctx) = make_ctx(ArchitectureType::HexagonalEventSourcedVsa, slices);
        slice(&ctx.root, "a", "[command, bogus]", &[]);
        slice(&ctx.root, "b", "[command, automation]", &[]);
        slice(&ctx.root, "c", "[command, mixed]", &[]);
        slice(&ctx.root, "d", "[command, query]", &[]);
        let (unknown, _) = codes(&UnknownSliceTypeRule, &ctx);
        assert_eq!(unknown.len(), 2, "{unknown:?}");
        assert!(unknown.iter().any(|e| e.contains("'a' declares unknown type 'bogus'")));
        assert!(unknown.iter().any(|e| e.contains("'c' lists 'mixed'")));
        let (undeclared, _) = codes(&UndeclaredSliceTypeRule, &ctx);
        assert_eq!(undeclared.len(), 2, "{undeclared:?}");
        assert!(undeclared.iter().any(|e| e.contains("'b' is type 'automation'")));
        assert!(undeclared.iter().any(|e| e.contains("'c' is type 'mixed'")));
        let (_, deprecated) = codes(&DeprecatedSliceTypeNameRule, &ctx);
        assert_eq!(deprecated.len(), 1, "{deprecated:?}");
        assert!(deprecated[0].contains("'d' uses type 'query'"));
    }

    #[test]
    fn a_secondary_view_needs_its_projection() {
        use crate::validation::RequireProjectionForQueryRule;
        let (_t, ctx) = hex();
        slice(&ctx.root, "place", "[command, view]", &[("place_handler.rs", HANDLER)]);
        let (errors, _) = codes(&RequireProjectionForQueryRule, &ctx);
        assert_eq!(errors.len(), 1, "{errors:?}");
    }

    #[test]
    fn a_secondary_translation_must_issue_a_command() {
        let (_t, ctx) = hex();
        slice(&ctx.root, "import", "[command, translation]", &[("webhook.rs", "pub fn on() {}\n")]);
        let (errors, _) = codes(&AutomationSliceShapeRule, &ctx);
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].starts_with("translation slice 'import'"), "{}", errors[0]);
    }

    #[test]
    fn a_view_listed_beside_a_command_issuer_is_still_a_view() {
        let (_t, ctx) = hex();
        slice(
            &ctx.root,
            "pending",
            "[automation, view]",
            &[("pending_projection.rs", TODO_PROJECTION), ("process.rs", PROCESSOR)],
        );
        let (errors, _) = codes(&ViewSliceIssuesNoCommandRule, &ctx);
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].contains("View slice 'pending' issues a command"));
    }

    #[test]
    fn python_and_typescript_literals_are_not_commands() {
        let (_t, ctx) = hex();
        slice(
            &ctx.root,
            "py_view",
            "view",
            &[("projection.py", "label = \"PlaceOrderCommand\"\n")],
        );
        slice(
            &ctx.root,
            "ts_view",
            "view",
            &[("TotalsProjection.ts", "const l = `PlaceOrderCommand`;\nconst u = 'https://x';\n")],
        );
        slice(
            &ctx.root,
            "ts_runner",
            "view",
            &[("RunnerProjection.ts", "const u = 'https://x'; send(new PlaceOrderCommand());\n")],
        );
        let (errors, _) = codes(&ViewSliceIssuesNoCommandRule, &ctx);
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].contains("'ts_runner'"));
    }

    #[test]
    fn projection_only_view_needs_no_handler_but_a_query_does() {
        use crate::validation::RequireHandlerForQueryRule;
        let (_t, ctx) = hex();
        slice(&ctx.root, "totals", "view", &[("totals_projection.rs", "pub struct T;\n")]);
        slice(
            &ctx.root,
            "get_totals",
            "view",
            &[
                ("totals_projection.rs", "pub struct T;\n"),
                ("get_totals_query.rs", "pub struct Q;\n"),
            ],
        );
        slice(&ctx.root, "legacy", "query", &[("totals_projection.rs", "pub struct T;\n")]);
        let (errors, _) = codes(&RequireHandlerForQueryRule, &ctx);
        assert_eq!(errors.len(), 2, "{errors:?}");
        assert!(errors.iter().any(|e| e.contains("'get_totals'")));
        assert!(errors.iter().any(|e| e.contains("'legacy'")));
    }

    #[test]
    fn translation_diagnostic_is_about_translation() {
        let (_t, ctx) = hex();
        slice(&ctx.root, "import_jira", "translation", &[("webhook.rs", "pub fn on() {}\n")]);
        let (errors, _) = codes(&AutomationSliceShapeRule, &ctx);
        assert!(
            errors[0].contains("A translation takes what another system reports"),
            "{}",
            errors[0]
        );
    }
}
