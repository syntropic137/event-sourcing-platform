//! Suggestions for fixing validation issues

use std::path::PathBuf;

/// A suggestion for fixing a validation issue
#[derive(Debug, Clone)]
pub struct Suggestion {
    pub message: String,
    pub action: SuggestionAction,
}

/// Action that can be taken to fix an issue
#[derive(Debug, Clone)]
pub enum SuggestionAction {
    /// Create a file
    CreateFile { path: PathBuf, template: Option<String> },

    /// Rename a file
    RenameFile { from: PathBuf, to: PathBuf },

    /// Move a file
    MoveFile { from: PathBuf, to: PathBuf },

    /// Delete a file
    DeleteFile { path: PathBuf },

    /// Update configuration
    UpdateConfig { key: String, value: String },

    /// Custom command to run
    RunCommand { command: String },

    /// No automated action available
    Manual { instructions: String },
}

impl Suggestion {
    /// Create a suggestion to create a missing file
    pub fn create_file(path: PathBuf, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            action: SuggestionAction::CreateFile { path, template: None },
        }
    }

    /// Create a suggestion to create a file with a template
    pub fn create_file_with_template(
        path: PathBuf,
        template: String,
        message: impl Into<String>,
    ) -> Self {
        Self {
            message: message.into(),
            action: SuggestionAction::CreateFile { path, template: Some(template) },
        }
    }

    /// Create a suggestion to rename a file
    pub fn rename_file(from: PathBuf, to: PathBuf, message: impl Into<String>) -> Self {
        Self { message: message.into(), action: SuggestionAction::RenameFile { from, to } }
    }

    /// Create a manual suggestion
    pub fn manual(instructions: impl Into<String>) -> Self {
        Self {
            message: "Manual action required".to_string(),
            action: SuggestionAction::Manual { instructions: instructions.into() },
        }
    }
}
