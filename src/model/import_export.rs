use std::{fmt, pin::Pin};

use futures_util::Stream;

use super::{Command, VariableCompletion};
use crate::{config::Theme, errors::Result, format_error, format_msg, process::ProcessOutput};

/// A unified data model to handle both commands and completions in a single stream
#[derive(Clone)]
pub enum ImportExportItem {
    Command(Command),
    Completion(VariableCompletion),
}

impl fmt::Display for ImportExportItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImportExportItem::Command(c) => c.fmt(f),
            ImportExportItem::Completion(c) => c.fmt(f),
        }
    }
}

/// Stream of results of [`ImportExportItem`]
pub type ImportExportStream = Pin<Box<dyn Stream<Item = Result<ImportExportItem>> + Send>>;

/// Statistics collected when importing
#[derive(Default)]
pub struct ImportStats {
    pub commands_imported: u64,
    pub commands_updated: u64,
    pub commands_skipped: u64,
    pub completions_imported: u64,
    pub completions_updated: u64,
    pub completions_skipped: u64,
}

/// Statistics collected when exporting
#[derive(Default)]
pub struct ExportStats {
    pub commands_exported: u64,
    pub completions_exported: u64,
    pub stdout: Option<String>,
}

impl ImportStats {
    /// Converts these statistics into a [ProcessOutput] with proper message
    pub fn into_output(self, theme: &Theme) -> ProcessOutput {
        let ImportStats {
            commands_imported,
            commands_updated,
            commands_skipped,
            completions_imported,
            completions_updated,
            completions_skipped,
        } = self;

        // Check if any operations were performed at all
        let total_actions = commands_imported
            + commands_updated
            + commands_skipped
            + completions_imported
            + completions_updated
            + completions_skipped;

        // If no actions occurred, it implies no items were found to process
        if total_actions == 0 {
            return ProcessOutput::fail().stderr(format_error!(theme, "No commands or completions were found"));
        }

        // Determine if any actual changes (imports or updates) were made
        let was_changed =
            commands_imported > 0 || commands_updated > 0 || completions_imported > 0 || completions_updated > 0;

        let message = if was_changed {
            // Build message parts for each action type to combine them naturally
            let mut imported_parts = Vec::with_capacity(2);
            if commands_imported > 0 {
                imported_parts.push(format!(
                    "{} new command{}",
                    commands_imported,
                    plural_s(commands_imported)
                ));
            }
            if completions_imported > 0 {
                imported_parts.push(format!(
                    "{} new completion{}",
                    completions_imported,
                    plural_s(completions_imported),
                ));
            }

            let mut updated_parts = Vec::with_capacity(2);
            if commands_updated > 0 {
                updated_parts.push(format!("{} command{}", commands_updated, plural_s(commands_updated)));
            }
            if completions_updated > 0 {
                updated_parts.push(format!(
                    "{} completion{}",
                    completions_updated,
                    plural_s(completions_updated)
                ));
            }

            let mut skipped_parts = Vec::with_capacity(2);
            if commands_skipped > 0 {
                skipped_parts.push(format!("{} command{}", commands_skipped, plural_s(commands_skipped)));
            }
            if completions_skipped > 0 {
                skipped_parts.push(format!(
                    "{} completion{}",
                    completions_skipped,
                    plural_s(completions_skipped)
                ));
            }

            // The primary message focuses on imports first, then updates
            let main_msg;
            let mut secondary_msg_parts = Vec::with_capacity(2);

            if !imported_parts.is_empty() {
                main_msg = format!("Imported {}", imported_parts.join(" and "));
                // If there were imports, updates are secondary information.
                if !updated_parts.is_empty() {
                    secondary_msg_parts.push(format!("{} updated", updated_parts.join(" and ")));
                }
            } else {
                // If there were no imports, updates become the primary message.
                main_msg = format!("Updated {}", updated_parts.join(" and "));
            }

            // Skipped items are always secondary information.
            if !skipped_parts.is_empty() {
                secondary_msg_parts.push(format!("{} already existed", skipped_parts.join(" and ")));
            }

            // Combine the main message with the styled, parenthesized secondary message.
            let secondary_msg = if !secondary_msg_parts.is_empty() {
                format!(" ({})", secondary_msg_parts.join("; "))
            } else {
                String::new()
            };

            format_msg!(theme, "{main_msg}{}", theme.secondary.apply(secondary_msg))
        } else {
            // This message is for when only skips occurred
            let mut skipped_parts = Vec::with_capacity(2);
            if commands_skipped > 0 {
                skipped_parts.push(format!("{} command{}", commands_skipped, plural_s(commands_skipped)));
            }
            if completions_skipped > 0 {
                skipped_parts.push(format!(
                    "{} completion{}",
                    completions_skipped,
                    plural_s(completions_skipped),
                ));
            }
            format!("No new changes; {} already existed", skipped_parts.join(" and "))
        };

        ProcessOutput::success().stderr(message)
    }
}

impl ExportStats {
    /// Converts these statistics into a [ProcessOutput] with a proper message
    pub fn into_output(self, theme: &Theme) -> ProcessOutput {
        let ExportStats {
            commands_exported,
            completions_exported,
            stdout,
        } = self;

        // If nothing was exported, return a failure message
        if commands_exported == 0 && completions_exported == 0 {
            return ProcessOutput::fail().stderr(format_error!(theme, "No commands or completions to export"));
        }

        // Build a message describing what was exported
        let mut parts = Vec::with_capacity(2);
        if commands_exported > 0 {
            parts.push(format!("{} command{}", commands_exported, plural_s(commands_exported)));
        }
        if completions_exported > 0 {
            parts.push(format!(
                "{} completion{}",
                completions_exported,
                plural_s(completions_exported)
            ));
        }

        let summary = parts.join(" and ");
        let stderr_msg = format_msg!(theme, "Exported {summary}");

        // Create the base success output
        let mut output = ProcessOutput::success().stderr(stderr_msg);

        // If there's content for stdout, attach it to the output
        if let Some(stdout_content) = stdout {
            output = output.stdout(stdout_content);
        }

        output
    }
}

/// A simple helper to return "s" for pluralization if the count is not 1.
fn plural_s(count: u64) -> &'static str {
    if count == 1 { "" } else { "s" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Theme;

    #[test]
    fn test_import_stats_into_output_no_actions() {
        let stats = ImportStats::default();
        let theme = Theme::default();
        let output = stats.into_output(&theme);

        if let ProcessOutput::Output(info) = output {
            assert!(info.failed);
            assert!(info.stdout.is_none());
            assert_eq!(
                strip_ansi(info.stderr.as_deref().unwrap()),
                "[Error] No commands or completions were found",
            );
        } else {
            panic!("Expected ProcessOutput::Output variant");
        }
    }

    #[test]
    fn test_import_stats_into_output_only_skipped() {
        let stats = ImportStats {
            commands_skipped: 5,
            completions_skipped: 2,
            ..Default::default()
        };
        let theme = Theme::default();
        let output = stats.into_output(&theme);

        if let ProcessOutput::Output(info) = output {
            assert!(!info.failed);
            assert!(info.stdout.is_none());
            assert_eq!(
                strip_ansi(info.stderr.as_deref().unwrap()),
                "No new changes; 5 commands and 2 completions already existed",
            );
        } else {
            panic!("Expected ProcessOutput::Output variant");
        }
    }

    #[test]
    fn test_import_stats_into_output_only_skipped_singular() {
        let stats = ImportStats {
            commands_skipped: 1,
            completions_skipped: 1,
            ..Default::default()
        };
        let theme = Theme::default();
        let output = stats.into_output(&theme);

        if let ProcessOutput::Output(info) = output {
            assert!(!info.failed);
            assert!(info.stdout.is_none());
            assert_eq!(
                strip_ansi(info.stderr.as_deref().unwrap()),
                "No new changes; 1 command and 1 completion already existed",
            );
        } else {
            panic!("Expected ProcessOutput::Output variant");
        }
    }

    #[test]
    fn test_import_stats_into_output_only_imports() {
        let stats = ImportStats {
            commands_imported: 1,
            completions_imported: 1,
            ..Default::default()
        };
        let theme = Theme::default();
        let output = stats.into_output(&theme);

        if let ProcessOutput::Output(info) = output {
            assert!(!info.failed);
            assert!(info.stdout.is_none());
            assert_eq!(
                strip_ansi(info.stderr.as_deref().unwrap()),
                "-> Imported 1 new command and 1 new completion",
            );
        } else {
            panic!("Expected ProcessOutput::Output variant");
        }
    }

    #[test]
    fn test_import_stats_into_output_only_updates() {
        let stats = ImportStats {
            commands_updated: 10,
            completions_updated: 1,
            ..Default::default()
        };
        let theme = Theme::default();
        let output = stats.into_output(&theme);

        if let ProcessOutput::Output(info) = output {
            assert!(!info.failed);
            assert!(info.stdout.is_none());
            assert_eq!(
                strip_ansi(info.stderr.as_deref().unwrap()),
                "-> Updated 10 commands and 1 completion",
            );
        } else {
            panic!("Expected ProcessOutput::Output variant");
        }
    }

    #[test]
    fn test_import_stats_into_output_imports_and_skipped() {
        let stats = ImportStats {
            commands_imported: 3,
            commands_skipped: 2,
            completions_imported: 4,
            completions_skipped: 1,
            ..Default::default()
        };
        let theme = Theme::default();
        let output = stats.into_output(&theme);

        if let ProcessOutput::Output(info) = output {
            assert!(!info.failed);
            assert!(info.stdout.is_none());
            assert_eq!(
                strip_ansi(info.stderr.as_deref().unwrap()),
                "-> Imported 3 new commands and 4 new completions (2 commands and 1 completion already existed)",
            );
        } else {
            panic!("Expected ProcessOutput::Output variant");
        }
    }

    #[test]
    fn test_import_stats_into_output_imports_cmds_skipped_completions() {
        let stats = ImportStats {
            commands_imported: 5,
            completions_skipped: 3,
            ..Default::default()
        };
        let theme = Theme::default();
        let output = stats.into_output(&theme);

        if let ProcessOutput::Output(info) = output {
            assert!(!info.failed);
            assert!(info.stdout.is_none());
            assert_eq!(
                strip_ansi(info.stderr.as_deref().unwrap()),
                "-> Imported 5 new commands (3 completions already existed)",
            );
        } else {
            panic!("Expected ProcessOutput::Output variant");
        }
    }

    #[test]
    fn test_export_stats_into_output_no_actions() {
        let stats = ExportStats::default();
        let theme = Theme::default();
        let output = stats.into_output(&theme);

        if let ProcessOutput::Output(info) = output {
            assert!(info.failed);
            assert!(info.stdout.is_none());
            assert_eq!(
                strip_ansi(info.stderr.as_deref().unwrap()),
                "[Error] No commands or completions to export"
            );
        } else {
            panic!("Expected ProcessOutput::Output variant");
        }
    }

    #[test]
    fn test_export_stats_into_output_only_commands_singular() {
        let stats = ExportStats {
            commands_exported: 1,
            ..Default::default()
        };
        let theme = Theme::default();
        let output = stats.into_output(&theme);

        if let ProcessOutput::Output(info) = output {
            assert!(!info.failed);
            assert!(info.stdout.is_none());
            assert_eq!(strip_ansi(info.stderr.as_deref().unwrap()), "-> Exported 1 command");
        } else {
            panic!("Expected ProcessOutput::Output variant");
        }
    }

    #[test]
    fn test_export_stats_into_output_only_completions_plural() {
        let stats = ExportStats {
            completions_exported: 10,
            ..Default::default()
        };
        let theme = Theme::default();
        let output = stats.into_output(&theme);

        if let ProcessOutput::Output(info) = output {
            assert!(!info.failed);
            assert!(info.stdout.is_none());
            assert_eq!(
                strip_ansi(info.stderr.as_deref().unwrap()),
                "-> Exported 10 completions"
            );
        } else {
            panic!("Expected ProcessOutput::Output variant");
        }
    }

    #[test]
    fn test_export_stats_into_output_both_commands_and_completions() {
        let stats = ExportStats {
            commands_exported: 5,
            completions_exported: 8,
            ..Default::default()
        };
        let theme = Theme::default();
        let output = stats.into_output(&theme);

        if let ProcessOutput::Output(info) = output {
            assert!(!info.failed);
            assert!(info.stdout.is_none());
            assert_eq!(
                strip_ansi(info.stderr.as_deref().unwrap()),
                "-> Exported 5 commands and 8 completions"
            );
        } else {
            panic!("Expected ProcessOutput::Output variant");
        }
    }

    // Helper to strip ANSI color codes for cleaner assertions
    fn strip_ansi(s: &str) -> String {
        String::from_utf8(strip_ansi_escapes::strip(s.as_bytes())).unwrap()
    }
}
