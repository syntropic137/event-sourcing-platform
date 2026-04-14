//! Structure validation rules for ADR-019 and ADR-020
//!
//! Validates that the codebase follows the canonical VSA structure:
//! - VSA020: Commands must be in domain/commands/
//! - VSA021: Events must be in domain/events/ (domain cohesion per ADR-019)
//! - VSA022: Aggregates must be in domain/aggregate_*/ folders (ADR-020)
//! - VSA023: Ports must be in ports/ folder with *Port suffix
//! - VSA024: Buses must be in infrastructure/buses/, NOT application/
//! - VSA025: All ports must end with Port suffix
//! - VSA026: Value objects should follow *ValueObjects.* pattern

use super::rules::ValidationRule;
use super::{EnhancedValidationReport, Severity, Suggestion, ValidationContext, ValidationIssue};
use crate::error::Result;
use crate::scanner::Scanner;
use std::path::Path;

// ============================================================================
// VSA020: Commands must be in domain/commands/
// ============================================================================

/// VSA020: All commands must be located in domain/commands/ directory
///
/// This enforces centralized command organization as defined in ADR-019.
/// Commands are domain contracts that define all write operations.
///
/// Invalid structure:
/// ```text
/// contexts/workflows/
///   └── slices/
///       └── create_workflow/
///           └── CreateWorkflowCommand.py  # ❌ Command in slice
/// ```text
///
/// Valid structure:
/// ```text
/// contexts/workflows/
///   ├── domain/
///   │   └── commands/
///   │       └── CreateWorkflowCommand.py  # ✅ Command in domain/commands/
///   └── slices/
///       └── create_workflow/
///           └── internal/
///               └── Handler.py            # ✅ Handler imports from domain
/// ```text
pub struct RequireCommandsInDomainRule;

impl RequireCommandsInDomainRule {
    fn is_command_file(&self, path: &Path, ctx: &ValidationContext) -> bool {
        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => return false,
        };

        let ext = ctx.config.file_extension();
        file_name.ends_with(&format!("Command.{ext}"))
    }
}

impl ValidationRule for RequireCommandsInDomainRule {
    fn name(&self) -> &str {
        "require-commands-in-domain"
    }

    fn code(&self) -> &str {
        "VSA020"
    }

    fn validate(
        &self,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        let scanner = Scanner::new(ctx.config.clone(), ctx.root.clone());
        let contexts = scanner.scan_contexts()?;

        for context in contexts {
            // Walk all files in context, looking for *Command.* files
            if let Ok(entries) = std::fs::read_dir(&context.path) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    self.check_directory(&path, &context.path, &context.name, ctx, report)?;
                }
            }
        }

        Ok(())
    }
}

impl RequireCommandsInDomainRule {
    #[allow(clippy::only_used_in_recursion)]
    fn check_directory(
        &self,
        path: &Path,
        context_path: &Path,
        context_name: &str,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        if !path.is_dir() {
            return Ok(());
        }

        let dir_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => return Ok(()),
        };

        // Skip domain folder itself (we're checking other locations)
        if dir_name == "domain" {
            return Ok(());
        }

        // Recursively check subdirectories
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                let entry_path = entry.path();

                if entry_path.is_dir() {
                    self.check_directory(&entry_path, context_path, context_name, ctx, report)?;
                } else if self.is_command_file(&entry_path, ctx) {
                    // Found a command file outside domain/commands/
                    let relative_path = entry_path
                        .strip_prefix(context_path)
                        .unwrap_or(&entry_path)
                        .to_string_lossy();

                    // Check if it's in domain/commands/ (allowed)
                    if relative_path.starts_with("domain/commands/")
                        || relative_path.starts_with("domain\\commands\\")
                    {
                        continue;
                    }

                    let suggested_path =
                        context_path.join("domain/commands").join(entry_path.file_name().unwrap());

                    report.errors.push(ValidationIssue {
                        path: entry_path.clone(),
                        code: self.code().to_string(),
                        severity: Severity::Error,
                        message: format!(
                            "Command file '{}' in context '{}' is not in domain/commands/ directory. \
                             All commands should be centralized in domain/commands/ as per ADR-019 \
                             (found at: {})",
                            entry_path.file_name().unwrap().to_string_lossy(),
                            context_name,
                            relative_path
                        ),
                        suggestions: vec![Suggestion::manual(format!(
                            "Move this command to domain/commands/\n\
                             Command: git mv {} {}",
                            entry_path.display(),
                            suggested_path.display()
                        ))],
                    });
                }
            }
        }

        Ok(())
    }
}

// ============================================================================
// VSA021: Events must be in domain/events/, NOT at context root
// ============================================================================

/// VSA021: Events must be in domain/events/, NOT at context root (events/)
///
/// As defined in ADR-019, events are domain concepts that express domain facts
/// and should be in the domain folder for domain cohesion, not at context root.
///
/// Invalid structure:
/// ```text
/// contexts/workflows/
///   ├── events/
///   │   └── WorkflowCreatedEvent.py  # ❌ Events at context root
///   └── domain/
///       └── WorkflowAggregate.py
/// ```text
///
/// Valid structure:
/// ```text
/// contexts/workflows/
///   └── domain/
///       ├── events/
///       │   ├── WorkflowCreatedEvent.py      # ✅ Events in domain/events/
///       │   ├── versioned/                   # ✅ Old versions
///       │   └── upcasters/                   # ✅ Migrations
///       └── WorkflowAggregate.py
/// ```text
pub struct RequireEventsInDomainRule;

impl RequireEventsInDomainRule {
    fn is_event_file(&self, path: &Path, ctx: &ValidationContext) -> bool {
        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => return false,
        };

        let ext = ctx.config.file_extension();
        file_name.ends_with(&format!("Event.{ext}"))
    }
}

impl ValidationRule for RequireEventsInDomainRule {
    fn name(&self) -> &str {
        "require-events-in-domain"
    }

    fn code(&self) -> &str {
        "VSA021"
    }

    fn validate(
        &self,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        let scanner = Scanner::new(ctx.config.clone(), ctx.root.clone());
        let contexts = scanner.scan_contexts()?;

        for context in contexts {
            // Per ADR-019 v2: Events should be in domain/events/ (domain cohesion)
            // Events ARE domain language - they express domain facts

            // Check for event files in slices or other wrong locations
            self.check_directory_for_misplaced_events(
                &context.path,
                &context.path,
                &context.name,
                ctx,
                report,
            )?;
        }

        Ok(())
    }
}

impl RequireEventsInDomainRule {
    #[allow(clippy::only_used_in_recursion)]
    fn check_directory_for_misplaced_events(
        &self,
        path: &Path,
        context_path: &Path,
        context_name: &str,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        if !path.is_dir() {
            return Ok(());
        }

        let dir_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => return Ok(()),
        };

        // Skip allowed locations
        // Only domain/events/ is allowed (per ADR-019: domain cohesion)
        if path == context_path.join("domain").join("events") {
            return Ok(());
        }
        if dir_name == "versioned" || dir_name == "upcasters" {
            return Ok(());
        }

        // Recursively check subdirectories
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                let entry_path = entry.path();

                if entry_path.is_dir() {
                    self.check_directory_for_misplaced_events(
                        &entry_path,
                        context_path,
                        context_name,
                        ctx,
                        report,
                    )?;
                } else if self.is_event_file(&entry_path, ctx) {
                    let relative_path = entry_path
                        .strip_prefix(context_path)
                        .unwrap_or(&entry_path)
                        .to_string_lossy();

                    // Check if in allowed location (domain/events/ only)
                    if relative_path.starts_with("domain/events/")
                        || relative_path.starts_with("domain\\events\\")
                        || relative_path.starts_with("domain/events\\")
                        || relative_path.starts_with("domain\\events/")
                    {
                        continue;
                    }

                    // Found misplaced event
                    let suggested_path = context_path
                        .join("domain")
                        .join("events")
                        .join(entry_path.file_name().unwrap());

                    report.errors.push(ValidationIssue {
                        path: entry_path.clone(),
                        code: self.code().to_string(),
                        severity: Severity::Error,
                        message: format!(
                            "Event file '{}' in context '{}' is not in domain/events/. \
                             Per ADR-019, events should be in domain/events/ directory for domain cohesion, not in {} \
                             (found at: {})",
                            entry_path.file_name().unwrap().to_string_lossy(),
                            context_name,
                            dir_name,
                            relative_path
                        ),
                        suggestions: vec![Suggestion::manual(format!(
                            "Move this event to domain/events/\n\
                             Command: git mv {} {}",
                            entry_path.display(),
                            suggested_path.display()
                        ))],
                    });
                }
            }
        }

        Ok(())
    }
}

// ============================================================================
// VSA022: Aggregates must be in domain/aggregate_*/ folders
// ============================================================================

/// VSA022: Aggregates must be in domain/aggregate_*/ folders
///
/// As per ADR-020, aggregates MUST be in `domain/aggregate_*/` folders.
/// This convention:
/// - Clearly defines aggregate boundaries
/// - Groups aggregate root with its entities and value objects
/// - Enables future growth within the consistency boundary
/// - Makes aggregates easily identifiable via folder naming
///
/// Invalid structures:
/// ```text
/// contexts/workflows/
///   └── _shared/
///       └── WorkflowAggregate.py  # ❌ Aggregate in _shared/
///
/// contexts/workflows/
///   └── domain/
///       └── WorkflowAggregate.py  # ❌ Aggregate at domain root (use folder!)
/// ```text
///
/// Valid structure:
/// ```text
/// contexts/orchestration/
///   └── domain/
///       ├── aggregate_workflow/
///       │   ├── WorkflowAggregate.py      # ✅ Aggregate root in named folder
///       │   └── value_objects.py          # ✅ Related value objects
///       └── aggregate_workspace/
///           ├── WorkspaceAggregate.py     # ✅ Another aggregate
///           └── IsolationHandle.py        # ✅ Entity within aggregate
/// ```text
pub struct RequireAggregatesInDomainRootRule;

impl RequireAggregatesInDomainRootRule {
    fn is_aggregate_file(&self, path: &Path, ctx: &ValidationContext) -> bool {
        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => return false,
        };

        let ext = ctx.config.file_extension();
        file_name.ends_with(&format!("Aggregate.{ext}"))
    }

    /// Extract aggregate name from filename (e.g., "WorkflowAggregate.py" -> "workflow")
    fn extract_aggregate_name(&self, path: &Path) -> String {
        path.file_name()
            .and_then(|n| n.to_str())
            .map(|name| {
                // Remove extension and "Aggregate" suffix
                name.split('.')
                    .next()
                    .unwrap_or(name)
                    .trim_end_matches("Aggregate")
                    .to_string()
            })
            .unwrap_or_default()
    }
}

impl ValidationRule for RequireAggregatesInDomainRootRule {
    fn name(&self) -> &str {
        "require-aggregates-in-aggregate-folders"
    }

    fn code(&self) -> &str {
        "VSA022"
    }

    fn validate(
        &self,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        let scanner = Scanner::new(ctx.config.clone(), ctx.root.clone());
        let contexts = scanner.scan_contexts()?;

        for context in contexts {
            let domain_path = context.path.join("domain");

            // Check for aggregates in _shared/ (wrong location)
            let shared_path = context.path.join("_shared");
            if shared_path.exists() && shared_path.is_dir() {
                if let Ok(entries) = std::fs::read_dir(&shared_path) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file() && self.is_aggregate_file(&path, ctx) {
                            let agg_name = self.extract_aggregate_name(&path);
                            let suggested_folder = domain_path.join(format!("aggregate_{}", agg_name.to_lowercase()));
                            let suggested_path = suggested_folder.join(path.file_name().unwrap());

                            report.errors.push(ValidationIssue {
                                path: path.clone(),
                                code: self.code().to_string(),
                                severity: Severity::Error,
                                message: format!(
                                    "Aggregate '{}' in context '{}' is in _shared/ directory. \
                                     As per ADR-020, aggregates must be in domain/aggregate_*/ folders.",
                                    path.file_name().unwrap().to_string_lossy(),
                                    context.name
                                ),
                                suggestions: vec![Suggestion::manual(format!(
                                    "Move aggregate to domain/aggregate_*/ folder\n\
                                     Commands:\n\
                                     mkdir -p {}\n\
                                     git mv {} {}",
                                    suggested_folder.display(),
                                    path.display(),
                                    suggested_path.display()
                                ))],
                            });
                        }
                    }
                }
            }

            // Check for aggregates at domain/ root (should be in aggregate_*/ folders)
            if domain_path.exists() && domain_path.is_dir() {
                if let Ok(entries) = std::fs::read_dir(&domain_path) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file() && self.is_aggregate_file(&path, ctx) {
                            let agg_name = self.extract_aggregate_name(&path);
                            let suggested_folder = domain_path.join(format!("aggregate_{}", agg_name.to_lowercase()));
                            let suggested_path = suggested_folder.join(path.file_name().unwrap());

                            report.errors.push(ValidationIssue {
                                path: path.clone(),
                                code: self.code().to_string(),
                                severity: Severity::Error,
                                message: format!(
                                    "Aggregate '{}' in context '{}' is at domain/ root. \
                                     As per ADR-020, aggregates must be in domain/aggregate_*/ folders \
                                     to define clear consistency boundaries.",
                                    path.file_name().unwrap().to_string_lossy(),
                                    context.name
                                ),
                                suggestions: vec![Suggestion::manual(format!(
                                    "Move aggregate to domain/aggregate_*/ folder\n\
                                     Commands:\n\
                                     mkdir -p {}\n\
                                     git mv {} {}",
                                    suggested_folder.display(),
                                    path.display(),
                                    suggested_path.display()
                                ))],
                            });
                        }
                    }
                }
            }

            // Also check for aggregates in wrong locations (slices, etc.)
            self.check_directory_for_misplaced_aggregates(
                &context.path,
                &context.path,
                &context.name,
                ctx,
                report,
            )?;
        }

        Ok(())
    }
}

impl RequireAggregatesInDomainRootRule {
    #[allow(clippy::only_used_in_recursion)]
    fn check_directory_for_misplaced_aggregates(
        &self,
        path: &Path,
        context_path: &Path,
        context_name: &str,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        if !path.is_dir() {
            return Ok(());
        }

        let dir_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => return Ok(()),
        };

        // Skip domain/ folder and its immediate children
        if dir_name == "domain" {
            // Only check if aggregates are NOT in domain root
            // (they should be directly in domain/, not in subfolders)
            // EXCEPTION: aggregate_* folders are allowed per ADR-020 (DDD convention)
            if let Ok(entries) = std::fs::read_dir(path) {
                for entry in entries.flatten() {
                    let entry_path = entry.path();
                    let subfolder_name = entry_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("");

                    if entry_path.is_dir()
                        && subfolder_name != "commands"
                        && subfolder_name != "queries"
                        && subfolder_name != "events"
                        && subfolder_name != "read_models"
                        && subfolder_name != "_shared"
                        && !subfolder_name.starts_with("aggregate_")
                    {
                        // Check subfolders of domain/ for misplaced aggregates
                        // (but NOT aggregate_* folders which are valid DDD convention)
                        self.check_subfolder_for_aggregates(
                            &entry_path,
                            context_path,
                            context_name,
                            ctx,
                            report,
                        )?;
                    }
                }
            }
            return Ok(());
        }

        // Skip _shared as we already checked it
        if dir_name == "_shared" {
            return Ok(());
        }

        // Recursively check other directories
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                let entry_path = entry.path();
                if entry_path.is_dir() {
                    self.check_directory_for_misplaced_aggregates(
                        &entry_path,
                        context_path,
                        context_name,
                        ctx,
                        report,
                    )?;
                }
            }
        }

        Ok(())
    }

    fn check_subfolder_for_aggregates(
        &self,
        path: &Path,
        context_path: &Path,
        context_name: &str,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                let entry_path = entry.path();

                if entry_path.is_file() && self.is_aggregate_file(&entry_path, ctx) {
                    let relative_path = entry_path
                        .strip_prefix(context_path)
                        .unwrap_or(&entry_path)
                        .to_string_lossy();

                    let domain_path = context_path.join("domain");
                    let suggested_path = domain_path.join(entry_path.file_name().unwrap());

                    report.errors.push(ValidationIssue {
                        path: entry_path.clone(),
                        code: self.code().to_string(),
                        severity: Severity::Error,
                        message: format!(
                            "Aggregate '{}' in context '{}' is in a subdirectory ({}). \
                             Aggregates should be directly in domain/ root, not in subfolders.",
                            entry_path.file_name().unwrap().to_string_lossy(),
                            context_name,
                            relative_path
                        ),
                        suggestions: vec![Suggestion::manual(format!(
                            "Move aggregate to domain/ root\n\
                             Command: git mv {} {}",
                            entry_path.display(),
                            suggested_path.display()
                        ))],
                    });
                }

                if entry_path.is_dir() {
                    self.check_subfolder_for_aggregates(
                        &entry_path,
                        context_path,
                        context_name,
                        ctx,
                        report,
                    )?;
                }
            }
        }

        Ok(())
    }
}

// ============================================================================
// VSA023: Ports must be in ports/ folder
// ============================================================================

/// VSA023: All port interfaces must be in ports/ folder
///
/// As per ADR-019, port interfaces define the hexagonal boundaries and
/// should be in a dedicated ports/ folder for discoverability.
///
/// Invalid:
/// ```text
/// contexts/workflows/
///   ├── domain/
///   │   └── WorkflowRepositoryPort.py    # ❌ Port in domain/
///   └── infrastructure/
///       └── CommandBusPort.py            # ❌ Port in infrastructure/
/// ```text
///
/// Valid:
/// ```text
/// contexts/workflows/
///   ├── ports/
///   │   ├── WorkflowRepositoryPort.py    # ✅ Port in ports/
///   │   └── CommandBusPort.py            # ✅ Port in ports/
///   └── infrastructure/
///       └── repositories/
///           └── PostgresWorkflowRepository.py  # ✅ Implementation
/// ```text
pub struct RequirePortsInPortsFolderRule;

impl RequirePortsInPortsFolderRule {
    fn is_port_file(&self, path: &Path, ctx: &ValidationContext) -> bool {
        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => return false,
        };

        let ext = ctx.config.file_extension();

        // Check file stem for Port suffix
        if let Some(file_stem) = path.file_stem().and_then(|s| s.to_str()) {
            return file_stem.ends_with("Port") && file_name.ends_with(&format!(".{ext}"));
        }

        false
    }
}

impl ValidationRule for RequirePortsInPortsFolderRule {
    fn name(&self) -> &str {
        "require-ports-in-ports-folder"
    }

    fn code(&self) -> &str {
        "VSA023"
    }

    fn validate(
        &self,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        let scanner = Scanner::new(ctx.config.clone(), ctx.root.clone());
        let contexts = scanner.scan_contexts()?;

        for context in contexts {
            // Check all directories except ports/ for port files
            self.check_directory(&context.path, &context.path, &context.name, ctx, report)?;
        }

        Ok(())
    }
}

impl RequirePortsInPortsFolderRule {
    #[allow(clippy::only_used_in_recursion)]
    fn check_directory(
        &self,
        path: &Path,
        context_path: &Path,
        context_name: &str,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        if !path.is_dir() {
            return Ok(());
        }

        let dir_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => return Ok(()),
        };

        // Skip ports/ folder itself (this is the correct location)
        if dir_name == "ports" && path == context_path.join("ports") {
            return Ok(());
        }

        // Check files in this directory
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                let entry_path = entry.path();

                if entry_path.is_dir() {
                    self.check_directory(&entry_path, context_path, context_name, ctx, report)?;
                } else if self.is_port_file(&entry_path, ctx) {
                    // Found a port file outside ports/ folder
                    let relative_path = entry_path
                        .strip_prefix(context_path)
                        .unwrap_or(&entry_path)
                        .to_string_lossy();

                    let ports_path = context_path.join("ports");
                    let suggested_path = ports_path.join(entry_path.file_name().unwrap());

                    report.errors.push(ValidationIssue {
                        path: entry_path.clone(),
                        code: self.code().to_string(),
                        severity: Severity::Error,
                        message: format!(
                            "Port interface '{}' in context '{}' is not in ports/ directory. \
                             As per ADR-019, all port interfaces should be in ports/ folder \
                             for discoverability and hexagonal boundary enforcement \
                             (found at: {})",
                            entry_path.file_name().unwrap().to_string_lossy(),
                            context_name,
                            relative_path
                        ),
                        suggestions: vec![Suggestion::manual(format!(
                            "Move this port to ports/ folder\n\
                             Commands:\n\
                             mkdir -p {}\n\
                             git mv {} {}",
                            ports_path.display(),
                            entry_path.display(),
                            suggested_path.display()
                        ))],
                    });
                }
            }
        }

        Ok(())
    }
}

// ============================================================================
// VSA024: Buses must be in infrastructure/buses/
// ============================================================================

/// VSA024: Buses must be in infrastructure/buses/, NOT application/
///
/// As per ADR-019, buses (CommandBus, EventBus, QueryBus) are infrastructure
/// concerns (message routing), not business orchestration.
///
/// Invalid:
/// ```text
/// contexts/workflows/
///   └── application/
///       ├── CommandBus.py      # ❌ Bus in application/
///       └── EventBus.py        # ❌ Bus in application/
/// ```text
///
/// Valid:
/// ```text
/// contexts/workflows/
///   ├── ports/
///   │   ├── CommandBusPort.py              # ✅ Interface in ports/
///   │   └── EventBusPort.py                # ✅ Interface in ports/
///   ├── application/
///   │   └── WorkflowSagaCoordinator.py     # ✅ Business orchestration
///   └── infrastructure/
///       └── buses/
///           ├── InMemoryCommandBus.py      # ✅ Implementation
///           └── InMemoryEventBus.py        # ✅ Implementation
/// ```text
pub struct RequireBusesInInfrastructureRule;

impl RequireBusesInInfrastructureRule {
    fn is_bus_file(&self, path: &Path, ctx: &ValidationContext) -> bool {
        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => return false,
        };

        let ext = ctx.config.file_extension();

        // Check for Bus in filename (but not Port - those are interfaces)
        if let Some(file_stem) = path.file_stem().and_then(|s| s.to_str()) {
            return file_stem.contains("Bus")
                && !file_stem.ends_with("Port")
                && file_name.ends_with(&format!(".{ext}"));
        }

        false
    }
}

impl ValidationRule for RequireBusesInInfrastructureRule {
    fn name(&self) -> &str {
        "require-buses-in-infrastructure"
    }

    fn code(&self) -> &str {
        "VSA024"
    }

    fn validate(
        &self,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        let scanner = Scanner::new(ctx.config.clone(), ctx.root.clone());
        let contexts = scanner.scan_contexts()?;

        for context in contexts {
            // Check specifically in application/ folder (common wrong location)
            let application_path = context.path.join("application");
            if application_path.exists() && application_path.is_dir() {
                self.check_application_for_buses(
                    &application_path,
                    &context.path,
                    &context.name,
                    ctx,
                    report,
                )?;
            }

            // Also check other wrong locations
            self.check_directory(&context.path, &context.path, &context.name, ctx, report)?;
        }

        Ok(())
    }
}

impl RequireBusesInInfrastructureRule {
    fn check_application_for_buses(
        &self,
        application_path: &Path,
        context_path: &Path,
        context_name: &str,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        if let Ok(entries) = std::fs::read_dir(application_path) {
            for entry in entries.flatten() {
                let path = entry.path();

                if path.is_file() && self.is_bus_file(&path, ctx) {
                    let infrastructure_buses_path = context_path.join("infrastructure/buses");
                    let suggested_path = infrastructure_buses_path.join(path.file_name().unwrap());

                    report.errors.push(ValidationIssue {
                        path: path.clone(),
                        code: self.code().to_string(),
                        severity: Severity::Error,
                        message: format!(
                            "Bus implementation '{}' in context '{}' is in application/ directory. \
                             As per ADR-019, buses are infrastructure concerns (message routing) \
                             and should be in infrastructure/buses/, not application/. \
                             Application layer is for business orchestration, not technical plumbing.",
                            path.file_name().unwrap().to_string_lossy(),
                            context_name
                        ),
                        suggestions: vec![Suggestion::manual(format!(
                            "Move bus to infrastructure/buses/\n\
                             Commands:\n\
                             mkdir -p {}\n\
                             git mv {} {}",
                            infrastructure_buses_path.display(),
                            path.display(),
                            suggested_path.display()
                        ))],
                    });
                }

                if path.is_dir() {
                    self.check_application_for_buses(
                        &path,
                        context_path,
                        context_name,
                        ctx,
                        report,
                    )?;
                }
            }
        }

        Ok(())
    }

    #[allow(clippy::only_used_in_recursion)]
    fn check_directory(
        &self,
        path: &Path,
        context_path: &Path,
        context_name: &str,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        if !path.is_dir() {
            return Ok(());
        }

        let dir_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => return Ok(()),
        };

        // Skip infrastructure/buses/ (correct location)
        if dir_name == "buses"
            && path.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str())
                == Some("infrastructure")
        {
            return Ok(());
        }

        // Skip ports/ (bus interfaces are allowed there)
        if dir_name == "ports" {
            return Ok(());
        }

        // Skip application/ (handled separately)
        if dir_name == "application" && path == context_path.join("application") {
            return Ok(());
        }

        // Check other locations
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                let entry_path = entry.path();
                if entry_path.is_dir() {
                    self.check_directory(&entry_path, context_path, context_name, ctx, report)?;
                }
            }
        }

        Ok(())
    }
}

// ============================================================================
// VSA026: Value objects naming convention
// ============================================================================

/// VSA026: Value objects should follow *ValueObjects.* pattern (if in separate file)
///
/// As per ADR-019, complex/reusable value objects in separate files should
/// use the *ValueObjects suffix for discoverability.
///
/// Invalid:
/// ```text
/// domain/WorkflowValues.py       # ❌ Missing ValueObjects suffix
/// domain/Values.py               # ❌ Too generic
/// ```text
///
/// Valid:
/// ```text
/// domain/WorkflowValueObjects.py # ✅ Clear naming
/// domain/WorkflowAggregate.py    # ✅ Inline value objects OK
/// ```text
pub struct RequireValueObjectsNamingRule;

impl ValidationRule for RequireValueObjectsNamingRule {
    fn name(&self) -> &str {
        "require-value-objects-naming"
    }

    fn code(&self) -> &str {
        "VSA026"
    }

    fn validate(
        &self,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        let scanner = Scanner::new(ctx.config.clone(), ctx.root.clone());
        let contexts = scanner.scan_contexts()?;

        for context in contexts {
            let domain_path = context.path.join("domain");
            if !domain_path.exists() || !domain_path.is_dir() {
                continue;
            }

            // Check files in domain/ root
            if let Ok(entries) = std::fs::read_dir(&domain_path) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if !path.is_file() {
                        continue;
                    }

                    let file_name = match path.file_name().and_then(|n| n.to_str()) {
                        Some(name) => name,
                        None => continue,
                    };

                    let ext = ctx.config.file_extension();
                    if !file_name.ends_with(&format!(".{ext}")) {
                        continue;
                    }

                    let file_stem = match path.file_stem().and_then(|n| n.to_str()) {
                        Some(stem) => stem,
                        None => continue,
                    };

                    // Skip aggregates, commands, events, queries
                    if file_stem.ends_with("Aggregate")
                        || file_stem.ends_with("Command")
                        || file_stem.ends_with("Event")
                        || file_stem.ends_with("Query")
                        || file_stem.ends_with("Port")
                    {
                        continue;
                    }

                    // Skip __init__, mod, etc.
                    if file_stem.starts_with("__") || file_stem == "mod" {
                        continue;
                    }

                    // Check for value object indicators without proper suffix
                    if (file_stem.contains("Value") || file_stem.contains("VO"))
                        && !file_stem.ends_with("ValueObjects")
                    {
                        let suggested_name = if file_stem.ends_with("s") {
                            format!("{file_stem}Objects.{ext}")
                        } else {
                            format!("{file_stem}ValueObjects.{ext}")
                        };

                        let suggested_path = domain_path.join(&suggested_name);

                        report.warnings.push(ValidationIssue {
                            path: path.clone(),
                            code: self.code().to_string(),
                            severity: Severity::Warning,
                            message: format!(
                                "File '{file_name}' in domain/ appears to contain value objects but doesn't \
                                 follow *ValueObjects naming convention. As per ADR-019, value object \
                                 files should use *ValueObjects suffix for discoverability."
                            ),
                            suggestions: vec![Suggestion::manual(format!(
                                "Consider renaming to {suggested_name}\n\
                                 Command: git mv {} {}",
                                path.display(),
                                suggested_path.display()
                            ))],
                        });
                    }
                }
            }
        }

        Ok(())
    }
}

// ============================================================================
// VSA025: All ports must end with Port suffix
// ============================================================================

/// VSA025: All interfaces in ports/ folder must end with Port suffix
///
/// As per ADR-019, all port interfaces must use the *Port naming convention
/// for discoverability and automated validation.
///
/// Invalid:
/// ```text
/// ports/WorkflowRepository.py      # ❌ Missing Port suffix
/// ports/CommandBus.py              # ❌ Missing Port suffix
/// ```text
///
/// Valid:
/// ```text
/// ports/WorkflowRepositoryPort.py  # ✅ Has Port suffix
/// ports/CommandBusPort.py          # ✅ Has Port suffix
/// ```text
pub struct RequirePortSuffixRule;

impl ValidationRule for RequirePortSuffixRule {
    fn name(&self) -> &str {
        "require-port-suffix"
    }

    fn code(&self) -> &str {
        "VSA025"
    }

    fn validate(
        &self,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        let scanner = Scanner::new(ctx.config.clone(), ctx.root.clone());
        let contexts = scanner.scan_contexts()?;

        for context in contexts {
            let ports_path = context.path.join("ports");
            if !ports_path.exists() || !ports_path.is_dir() {
                continue;
            }

            // Check all files in ports/ directory
            if let Ok(entries) = std::fs::read_dir(&ports_path) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if !path.is_file() {
                        continue;
                    }

                    let file_name = match path.file_name().and_then(|n| n.to_str()) {
                        Some(name) => name,
                        None => continue,
                    };

                    let ext = ctx.config.file_extension();
                    if !file_name.ends_with(&format!(".{ext}")) {
                        continue;
                    }

                    // Get file stem (without extension)
                    let file_stem = match path.file_stem().and_then(|n| n.to_str()) {
                        Some(stem) => stem,
                        None => continue,
                    };

                    // Skip __init__.py and similar
                    if file_stem.starts_with("__") || file_stem == "mod" {
                        continue;
                    }

                    // Check for Port suffix
                    if !file_stem.ends_with("Port") {
                        let suggested_name = format!("{file_stem}Port.{ext}");
                        let suggested_path = ports_path.join(&suggested_name);

                        report.errors.push(ValidationIssue {
                            path: path.clone(),
                            code: self.code().to_string(),
                            severity: Severity::Error,
                            message: format!(
                                "Port file '{}' in context '{}' does not end with 'Port' suffix. \
                                 As per ADR-019, all port interfaces must use *Port naming \
                                 for discoverability and consistency.",
                                file_name, context.name
                            ),
                            suggestions: vec![Suggestion::manual(format!(
                                "Rename to {suggested_name}\n\
                                 Command: git mv {} {}",
                                path.display(),
                                suggested_path.display()
                            ))],
                        });
                    }
                }
            }
        }

        Ok(())
    }
}

// ============================================================================
// VSA027: Aggregate folder convention
// ============================================================================

/// VSA027: Aggregates should follow the aggregate_<name>/ folder convention
///
/// As per ADR-020, each aggregate should live in its own folder:
/// - Folder name: `aggregate_<name>/` (lowercase, snake_case)
/// - One `*Aggregate.*` file per folder (the aggregate root)
/// - Entities and value objects co-located in the same folder
///
/// Invalid structure:
/// ```text
/// domain/
///   ├── WorkspaceAggregate.py       # ❌ Not in aggregate_* folder
///   ├── WorkflowAggregate.py        # ❌ Not in aggregate_* folder
///   └── aggregate_workspace/
///       ├── WorkspaceAggregate.py
///       └── OtherAggregate.py       # ❌ Multiple aggregates in one folder
/// ```text
///
/// Valid structure:
/// ```text
/// domain/
///   ├── aggregate_workspace/
///   │   ├── WorkspaceAggregate.py   # ✅ One aggregate per folder
///   │   ├── IsolationHandle.py      # ✅ Entity co-located
///   │   └── SecurityPolicy.py       # ✅ Value object co-located
///   └── aggregate_workflow/
///       └── WorkflowAggregate.py    # ✅ One aggregate per folder
/// ```text
pub struct RequireAggregateFolderConventionRule;

impl RequireAggregateFolderConventionRule {
    fn is_aggregate_file(&self, path: &Path, ctx: &ValidationContext) -> bool {
        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => return false,
        };

        let ext = ctx.config.file_extension();
        file_name.ends_with(&format!("Aggregate.{ext}"))
    }
}

impl ValidationRule for RequireAggregateFolderConventionRule {
    fn name(&self) -> &str {
        "require-aggregate-folder-convention"
    }

    fn code(&self) -> &str {
        "VSA027"
    }

    fn validate(
        &self,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        let scanner = Scanner::new(ctx.config.clone(), ctx.root.clone());
        let contexts = scanner.scan_contexts()?;

        for context in contexts {
            let domain_path = context.path.join("domain");
            if !domain_path.exists() || !domain_path.is_dir() {
                continue;
            }

            // NOTE: Aggregates at domain/ root are now checked by VSA022 (as error)
            // VSA027 only checks aggregate_* folder contents

            // Check for multiple aggregates in same aggregate_* folder
            self.check_aggregate_folder_contents(&domain_path, &context.name, ctx, report)?;
        }

        Ok(())
    }
}

impl RequireAggregateFolderConventionRule {
    fn check_aggregates_in_domain_root(
        &self,
        domain_path: &Path,
        context_name: &str,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        if let Ok(entries) = std::fs::read_dir(domain_path) {
            for entry in entries.flatten() {
                let path = entry.path();

                if path.is_file() && self.is_aggregate_file(&path, ctx) {
                    let file_name = path.file_name().unwrap().to_string_lossy();

                    // Extract aggregate name from file name (e.g., "WorkspaceAggregate.py" -> "workspace")
                    let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                    let aggregate_name =
                        file_stem.strip_suffix("Aggregate").unwrap_or(file_stem).to_lowercase();

                    let suggested_folder = format!("aggregate_{aggregate_name}");
                    let suggested_path = domain_path.join(&suggested_folder).join(&*file_name);

                    report.warnings.push(ValidationIssue {
                        path: path.clone(),
                        code: self.code().to_string(),
                        severity: Severity::Warning,
                        message: format!(
                            "Aggregate '{file_name}' in context '{context_name}' is directly in domain/ root. \
                             Per ADR-020, aggregates should be in aggregate_<name>/ folders \
                             for better organization and co-location of related entities/VOs."
                        ),
                        suggestions: vec![Suggestion::manual(format!(
                            "Move to aggregate_* folder:\n\
                             mkdir -p {}\n\
                             git mv {} {}",
                            domain_path.join(&suggested_folder).display(),
                            path.display(),
                            suggested_path.display()
                        ))],
                    });
                }
            }
        }

        Ok(())
    }

    fn check_aggregate_folder_contents(
        &self,
        domain_path: &Path,
        context_name: &str,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        if let Ok(entries) = std::fs::read_dir(domain_path) {
            for entry in entries.flatten() {
                let path = entry.path();

                if path.is_dir() {
                    let folder_name = match path.file_name().and_then(|n| n.to_str()) {
                        Some(name) => name,
                        None => continue,
                    };

                    // Check aggregate_* folders for multiple aggregates
                    if folder_name.starts_with("aggregate_") {
                        let mut aggregate_files = Vec::new();

                        if let Ok(folder_entries) = std::fs::read_dir(&path) {
                            for folder_entry in folder_entries.flatten() {
                                let file_path = folder_entry.path();
                                if file_path.is_file() && self.is_aggregate_file(&file_path, ctx) {
                                    aggregate_files.push(file_path);
                                }
                            }
                        }

                        // Check for multiple aggregates in one folder
                        if aggregate_files.len() > 1 {
                            report.errors.push(ValidationIssue {
                                path: path.clone(),
                                code: self.code().to_string(),
                                severity: Severity::Error,
                                message: format!(
                                    "Folder '{}' in context '{}' contains {} aggregate files. \
                                     Per ADR-020, each aggregate_* folder should have exactly ONE *Aggregate file. \
                                     Files found: {}",
                                    folder_name,
                                    context_name,
                                    aggregate_files.len(),
                                    aggregate_files
                                        .iter()
                                        .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                ),
                                suggestions: vec![Suggestion::manual(
                                    "Each aggregate should have its own aggregate_* folder. \
                                     Move extra aggregates to their own folders.".to_string()
                                )],
                            });
                        }

                        // Check for empty aggregate_* folder (no aggregate file)
                        if aggregate_files.is_empty() {
                            // Check if folder has any files at all
                            let has_files = std::fs::read_dir(&path)
                                .map(|entries| {
                                    entries.filter_map(|e| e.ok()).any(|e| e.path().is_file())
                                })
                                .unwrap_or(false);

                            if has_files {
                                report.warnings.push(ValidationIssue {
                                path: path.clone(),
                                code: self.code().to_string(),
                                severity: Severity::Warning,
                                message: format!(
                                    "Folder '{folder_name}' in context '{context_name}' is named like an aggregate folder \
                                     but contains no *Aggregate file. \
                                     Per ADR-020, each aggregate_* folder should contain its aggregate root."
                                ),
                                    suggestions: vec![Suggestion::manual(
                                        "Add the aggregate root file to this folder or rename the folder.".to_string()
                                    )],
                                });
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::VsaConfig;
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn create_test_config(root: PathBuf, language: &str) -> VsaConfig {
        VsaConfig {
            version: 2,
            architecture: crate::config::ArchitectureType::HexagonalEventSourcedVsa,
            root,
            language: language.to_string(),
            domain: None,
            slices: None,
            infrastructure: None,
            framework: None,
            contexts: HashMap::new(),
            validation: crate::config::ValidationConfig::default(),
            patterns: crate::config::PatternsConfig::default(),
            projection_allowed_prefixes: None,
        }
    }

    #[test]
    fn test_vsa020_command_in_slice() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        let context_path = root.join("workflows");
        let slice_path = context_path.join("slices/create_workflow");
        fs::create_dir_all(&slice_path).unwrap();

        // Command in slice (wrong location)
        fs::write(slice_path.join("CreateWorkflowCommand.py"), "class CreateWorkflowCommand: pass")
            .unwrap();

        let config = create_test_config(root.clone(), "python");
        let ctx = ValidationContext::new(config, root);
        let mut report = EnhancedValidationReport::default();

        let rule = RequireCommandsInDomainRule;
        rule.validate(&ctx, &mut report).unwrap();

        assert_eq!(report.errors.len(), 1);
        assert_eq!(report.errors[0].code, "VSA020");
        assert!(report.errors[0].message.contains("domain/commands/"));
    }

    /// Test VSA021: Events in domain/events/ is the correct location per ADR-019
    /// This validates that events are properly placed in the domain folder for domain cohesion
    #[test]
    fn test_vsa021_events_in_domain_events_correct() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        let context_path = root.join("workflows");
        let domain_events_path = context_path.join("domain/events");
        fs::create_dir_all(&domain_events_path).unwrap();

        // Create vsa.yaml with domain/events configuration
        fs::write(context_path.join("vsa.yaml"), "events_path: \"domain/events\"\n").unwrap();

        // Event in domain/events/ per ADR-019 (correct canonical location)
        fs::write(
            domain_events_path.join("WorkflowCreatedEvent.py"),
            "class WorkflowCreatedEvent: pass",
        )
        .unwrap();

        let config = create_test_config(root.clone(), "python");
        let ctx = ValidationContext::new(config, root);
        let mut report = EnhancedValidationReport::default();

        let rule = RequireEventsInDomainRule;
        rule.validate(&ctx, &mut report).unwrap();

        // Should have 0 errors - domain/events/ is the canonical location per ADR-019
        assert_eq!(
            report.errors.len(),
            0,
            "Events in domain/events/ should be allowed per ADR-019. Errors: {:?}",
            report.errors
        );
    }

    #[test]
    fn test_vsa022_aggregate_in_shared() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        let context_path = root.join("workflows");
        let shared_path = context_path.join("_shared");
        fs::create_dir_all(&shared_path).unwrap();

        // Aggregate in _shared/ (wrong location)
        fs::write(shared_path.join("WorkflowAggregate.py"), "class WorkflowAggregate: pass")
            .unwrap();

        let config = create_test_config(root.clone(), "python");
        let ctx = ValidationContext::new(config, root);
        let mut report = EnhancedValidationReport::default();

        let rule = RequireAggregatesInDomainRootRule;
        rule.validate(&ctx, &mut report).unwrap();

        assert_eq!(report.errors.len(), 1);
        assert_eq!(report.errors[0].code, "VSA022");
        assert!(report.errors[0].message.contains("domain/aggregate_*/"));
    }

    #[test]
    fn test_vsa025_port_without_suffix() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        let context_path = root.join("workflows");
        let ports_path = context_path.join("ports");
        fs::create_dir_all(&ports_path).unwrap();

        // Port without Port suffix (wrong naming)
        fs::write(ports_path.join("WorkflowRepository.py"), "class WorkflowRepository: pass")
            .unwrap();

        let config = create_test_config(root.clone(), "python");
        let ctx = ValidationContext::new(config, root);
        let mut report = EnhancedValidationReport::default();

        let rule = RequirePortSuffixRule;
        rule.validate(&ctx, &mut report).unwrap();

        assert_eq!(report.errors.len(), 1);
        assert_eq!(report.errors[0].code, "VSA025");
        assert!(report.errors[0].message.contains("Port"));
    }

    #[test]
    fn test_vsa023_port_outside_ports_folder() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        let context_path = root.join("workflows");
        let domain_path = context_path.join("domain");
        fs::create_dir_all(&domain_path).unwrap();

        // Port in domain/ (wrong location)
        fs::write(
            domain_path.join("WorkflowRepositoryPort.py"),
            "class WorkflowRepositoryPort: pass",
        )
        .unwrap();

        let config = create_test_config(root.clone(), "python");
        let ctx = ValidationContext::new(config, root);
        let mut report = EnhancedValidationReport::default();

        let rule = RequirePortsInPortsFolderRule;
        rule.validate(&ctx, &mut report).unwrap();

        assert_eq!(report.errors.len(), 1);
        assert_eq!(report.errors[0].code, "VSA023");
        assert!(report.errors[0].message.contains("ports/ folder"));
    }

    #[test]
    fn test_vsa024_bus_in_application() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        let context_path = root.join("workflows");
        let application_path = context_path.join("application");
        fs::create_dir_all(&application_path).unwrap();

        // Bus in application/ (wrong location)
        fs::write(application_path.join("CommandBus.py"), "class CommandBus: pass").unwrap();

        let config = create_test_config(root.clone(), "python");
        let ctx = ValidationContext::new(config, root);
        let mut report = EnhancedValidationReport::default();

        let rule = RequireBusesInInfrastructureRule;
        rule.validate(&ctx, &mut report).unwrap();

        assert_eq!(report.errors.len(), 1);
        assert_eq!(report.errors[0].code, "VSA024");
        assert!(report.errors[0].message.contains("infrastructure/buses/"));
    }

    #[test]
    fn test_vsa026_value_objects_naming() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        let context_path = root.join("workflows");
        let domain_path = context_path.join("domain");
        fs::create_dir_all(&domain_path).unwrap();

        // Value objects file without proper suffix (warning)
        fs::write(domain_path.join("WorkflowValues.py"), "class WorkflowId: pass").unwrap();

        let config = create_test_config(root.clone(), "python");
        let ctx = ValidationContext::new(config, root);
        let mut report = EnhancedValidationReport::default();

        let rule = RequireValueObjectsNamingRule;
        rule.validate(&ctx, &mut report).unwrap();

        assert_eq!(report.warnings.len(), 1);
        assert_eq!(report.warnings[0].code, "VSA026");
        assert!(report.warnings[0].message.contains("ValueObjects"));
    }

    #[test]
    fn test_valid_structure() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        let context_path = root.join("workflows");

        // Create valid structure
        let domain_path = context_path.join("domain");
        let commands_path = domain_path.join("commands");
        let events_path = domain_path.join("events");
        let ports_path = context_path.join("ports");
        let infrastructure_path = context_path.join("infrastructure");
        let buses_path = infrastructure_path.join("buses");

        let aggregate_path = domain_path.join("aggregate_workflow");
        fs::create_dir_all(&commands_path).unwrap();
        fs::create_dir_all(&events_path).unwrap();
        fs::create_dir_all(&ports_path).unwrap();
        fs::create_dir_all(&buses_path).unwrap();
        fs::create_dir_all(&aggregate_path).unwrap();

        // Aggregate in domain/aggregate_workflow/ folder (correct per ADR-020)
        fs::write(aggregate_path.join("WorkflowAggregate.py"), "class WorkflowAggregate: pass")
            .unwrap();

        // Command in domain/commands/ (correct)
        fs::write(
            commands_path.join("CreateWorkflowCommand.py"),
            "class CreateWorkflowCommand: pass",
        )
        .unwrap();

        // Event in domain/events/ (correct per ADR-019)
        fs::write(events_path.join("WorkflowCreatedEvent.py"), "class WorkflowCreatedEvent: pass")
            .unwrap();

        // Port with Port suffix in ports/ folder (correct)
        fs::write(
            ports_path.join("WorkflowRepositoryPort.py"),
            "class WorkflowRepositoryPort: pass",
        )
        .unwrap();

        // Bus in infrastructure/buses/ (correct)
        fs::write(buses_path.join("InMemoryCommandBus.py"), "class InMemoryCommandBus: pass")
            .unwrap();

        let config = create_test_config(root.clone(), "python");
        let ctx = ValidationContext::new(config, root);
        let mut report = EnhancedValidationReport::default();

        // Run all rules
        RequireCommandsInDomainRule.validate(&ctx, &mut report).unwrap();
        RequireEventsInDomainRule.validate(&ctx, &mut report).unwrap();
        RequireAggregatesInDomainRootRule.validate(&ctx, &mut report).unwrap();
        RequirePortsInPortsFolderRule.validate(&ctx, &mut report).unwrap();
        RequirePortSuffixRule.validate(&ctx, &mut report).unwrap();
        RequireBusesInInfrastructureRule.validate(&ctx, &mut report).unwrap();
        RequireValueObjectsNamingRule.validate(&ctx, &mut report).unwrap();

        assert_eq!(report.errors.len(), 0, "Valid structure should have no errors");
        assert_eq!(report.warnings.len(), 0, "Valid structure should have no warnings");
    }

    // ============================================================================
    // VSA027: Aggregate folder convention tests
    // ============================================================================

    #[test]
    fn test_vsa027_aggregate_not_in_folder_deferred_to_vsa022() {
        // NOTE: Aggregates at domain/ root are now checked by VSA022 (as error)
        // VSA027 no longer checks for this - it only validates aggregate_* folder contents
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        let context_path = root.join("workflows");
        let domain_path = context_path.join("domain");
        fs::create_dir_all(&domain_path).unwrap();

        // Aggregate directly in domain/ (not in aggregate_* folder)
        // VSA027 should NOT warn - this is handled by VSA022
        fs::write(domain_path.join("WorkflowAggregate.py"), "class WorkflowAggregate: pass")
            .unwrap();

        let config = create_test_config(root.clone(), "python");
        let ctx = ValidationContext::new(config, root);
        let mut report = EnhancedValidationReport::default();

        let rule = RequireAggregateFolderConventionRule;
        rule.validate(&ctx, &mut report).unwrap();

        // VSA027 no longer checks domain root - deferred to VSA022
        assert_eq!(report.warnings.len(), 0);
        assert_eq!(report.errors.len(), 0);
    }

    #[test]
    fn test_vsa027_multiple_aggregates_in_folder() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        let context_path = root.join("workflows");
        let aggregate_folder = context_path.join("domain/aggregate_workspace");
        fs::create_dir_all(&aggregate_folder).unwrap();

        // Two aggregates in same folder (error)
        fs::write(aggregate_folder.join("WorkspaceAggregate.py"), "class WorkspaceAggregate: pass")
            .unwrap();
        fs::write(aggregate_folder.join("OtherAggregate.py"), "class OtherAggregate: pass")
            .unwrap();

        let config = create_test_config(root.clone(), "python");
        let ctx = ValidationContext::new(config, root);
        let mut report = EnhancedValidationReport::default();

        let rule = RequireAggregateFolderConventionRule;
        rule.validate(&ctx, &mut report).unwrap();

        assert_eq!(report.errors.len(), 1);
        assert_eq!(report.errors[0].code, "VSA027");
        assert!(report.errors[0].message.contains("2 aggregate files"));
    }

    #[test]
    fn test_vsa027_valid_aggregate_folder() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        let context_path = root.join("workflows");
        let aggregate_folder = context_path.join("domain/aggregate_workspace");
        fs::create_dir_all(&aggregate_folder).unwrap();

        // Valid: One aggregate per folder with co-located entities
        fs::write(aggregate_folder.join("WorkspaceAggregate.py"), "class WorkspaceAggregate: pass")
            .unwrap();
        fs::write(aggregate_folder.join("IsolationHandle.py"), "class IsolationHandle: pass")
            .unwrap();
        fs::write(aggregate_folder.join("SecurityPolicy.py"), "class SecurityPolicy: pass")
            .unwrap();

        let config = create_test_config(root.clone(), "python");
        let ctx = ValidationContext::new(config, root);
        let mut report = EnhancedValidationReport::default();

        let rule = RequireAggregateFolderConventionRule;
        rule.validate(&ctx, &mut report).unwrap();

        assert_eq!(report.errors.len(), 0);
        assert_eq!(report.warnings.len(), 0);
    }

    #[test]
    fn test_vsa027_empty_aggregate_folder() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        let context_path = root.join("workflows");
        let aggregate_folder = context_path.join("domain/aggregate_workspace");
        fs::create_dir_all(&aggregate_folder).unwrap();

        // aggregate_* folder with files but no aggregate
        fs::write(aggregate_folder.join("IsolationHandle.py"), "class IsolationHandle: pass")
            .unwrap();

        let config = create_test_config(root.clone(), "python");
        let ctx = ValidationContext::new(config, root);
        let mut report = EnhancedValidationReport::default();

        let rule = RequireAggregateFolderConventionRule;
        rule.validate(&ctx, &mut report).unwrap();

        assert_eq!(report.warnings.len(), 1);
        assert_eq!(report.warnings[0].code, "VSA027");
        assert!(report.warnings[0].message.contains("no *Aggregate file"));
    }
}
