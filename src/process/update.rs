use std::borrow::Cow;

use semver::Version;
use tokio_util::sync::CancellationToken;

use crate::{
    cli::UpdateProcess,
    config::{Config, Theme},
    errors::AppError,
    format_error,
    model::IntelliShellRelease,
    process::{Process, ProcessOutput},
    service::IntelliShellService,
    utils::{InstallationMethod, VersionExt, detect_installation_method, render_markdown_to_ansi},
};

impl Process for UpdateProcess {
    async fn execute(
        self,
        config: Config,
        service: IntelliShellService,
        cancellation_token: CancellationToken,
    ) -> color_eyre::Result<ProcessOutput> {
        let current_version_str = env!("CARGO_PKG_VERSION");
        let current_version_tag = format!("v{current_version_str}");
        let current_version = crate::service::CURRENT_VERSION.clone();

        // Force fetch to ensure we have latest data
        let mut releases = match service.get_or_fetch_releases(true, cancellation_token).await {
            Ok(r) => r,
            Err(AppError::UserFacing(err)) => {
                return Ok(ProcessOutput::fail().stderr(format_error!(config.theme, "{err}")));
            }
            Err(AppError::Unexpected(report)) => return Err(report),
        };

        // Check if latest is newer than current
        let latest_version = match releases.first() {
            Some(r) if r.version > current_version => r.version.clone(),
            _ => {
                return Ok(ProcessOutput::success().stdout(format!(
                    "You're all set! You are running the latest version of intelli-shell ({}).",
                    config.theme.accent.apply(current_version_tag)
                )));
            }
        };

        // Common header for all update-needed messages
        let header = format!(
            "🚀 A new version is available! ({} -> {})",
            config.theme.secondary.apply(current_version_tag),
            config.theme.accent.apply(latest_version.to_tag()),
        );

        // Detect the installation method to provide tailored instructions
        match detect_installation_method(&config.data_dir) {
            // Handle automatic update via the installer
            InstallationMethod::Installer => {
                println!("{header}\n\nDownloading ...");

                let target_version_tag = latest_version.to_tag();
                let status = tokio::task::spawn_blocking(move || {
                    self_update::backends::github::Update::configure()
                        .repo_owner("lasantosr")
                        .repo_name("intelli-shell")
                        .bin_name("intelli-shell")
                        .show_output(false)
                        .show_download_progress(true)
                        .no_confirm(true)
                        .current_version(current_version_str)
                        .target_version_tag(&target_version_tag)
                        .build()?
                        .update()
                })
                .await?;

                println!("\n");

                // Provide update feedback
                match status {
                    Ok(self_update::Status::UpToDate(_)) => unreachable!(),
                    Ok(self_update::Status::Updated(_)) => {
                        // If the current version is not present, there has been a gap
                        let gap = !releases.iter().any(|r| r.version == current_version);
                        // We don't need considerations for the current version or older
                        releases.retain(|r| r.version > current_version);
                        // Build aggregated considerations message
                        let considerations = build_considerations_message(&releases, &latest_version);

                        // Build the final message
                        let mut msg = format!(
                            "✅ You're all set! You are now on intelli-shell {}.\n\n",
                            config.theme.accent.apply(latest_version.to_tag())
                        );
                        if !considerations.is_empty() {
                            if gap {
                                msg.push_str(
                                    "⚠️ You have skipped many versions. The following migration steps are required, \
                                     but please check GitHub Releases as this list may be incomplete:\n\n",
                                );
                            } else {
                                msg.push_str("💡 Some updates require additional steps to complete:\n\n");
                            }
                        } else if gap {
                            msg.push_str(
                                "⚠️ You have skipped many versions, please check GitHub Releases to ensure no manual \
                                 migration steps were missed.\n\n",
                            );
                        }
                        if !considerations.is_empty() {
                            msg.push_str(&render_markdown_to_ansi(&considerations, &config.theme));
                            msg.push_str("\n\n");
                        }
                        msg.push_str(&format!(
                            "📄 To view the full changelog, run: {}",
                            config
                                .theme
                                .accent
                                .apply(format!("intelli-shell changelog --from {}", current_version_str))
                        ));
                        Ok(ProcessOutput::success().stdout(msg))
                    }
                    Err(err) => Ok(ProcessOutput::fail().stderr(format!(
                        "❌ Update failed:\n{err}\n\nPlease check your network connection or file permissions.",
                    ))),
                }
            }
            // Provide clear, copyable instructions for other installation methods
            installation_method => {
                let instructions = get_manual_update_instructions(installation_method, &config.theme);
                let full_message = format!("{header}\n\n{instructions}");
                Ok(ProcessOutput::success().stdout(full_message))
            }
        }
    }
}

/// Generates user-friendly update instructions based on the installation method
fn get_manual_update_instructions(method: InstallationMethod, theme: &Theme) -> String {
    match method {
        InstallationMethod::Cargo => format!(
            "It looks like you installed with {}. To update, please run:\n\n{}\n",
            theme.secondary.apply("cargo"),
            theme
                .accent
                .apply("  LIBSQLITE3_FLAGS=\"-DSQLITE_ENABLE_MATH_FUNCTIONS\" cargo install intelli-shell --locked")
        ),
        InstallationMethod::Nix => format!(
            "It looks like you installed with {}. Consider updating it via your Nix configuration.",
            theme.secondary.apply("Nix")
        ),
        InstallationMethod::Source => format!(
            "It looks like you installed from {}. You might need to run:\n\n{}\n",
            theme.secondary.apply("source"),
            theme.accent.apply("  git pull && cargo build --release")
        ),
        InstallationMethod::Unknown(Some(path)) => format!(
            "Could not determine the installation method. Your executable is located at:\n\n  {}\n\nPlease update \
             manually or consider reinstalling with the recommended script.",
            theme.accent.apply(path)
        ),
        InstallationMethod::Unknown(None) => {
            "Could not determine the installation method. Please update manually.".to_string()
        }
        InstallationMethod::Installer => unreachable!(),
    }
}

/// Builds the aggregated considerations message from a list of releases.
///
/// `releases` is expected to be ordered Newest -> Oldest (as returned by `get_or_fetch_releases`).
fn build_considerations_message(releases: &[IntelliShellRelease], latest_version: &Version) -> String {
    // Collect only releases that have considerations
    let mut active_updates: Vec<(&IntelliShellRelease, String)> = releases
        .iter()
        .filter_map(|r| {
            r.body
                .as_deref()
                .and_then(extract_update_considerations)
                .map(|c| (r, c))
        })
        .collect();

    // Check if there's a single version with consideration, being the latest
    let is_single_latest =
        active_updates.len() == 1 && active_updates.first().map(|(r, _)| &r.version) == Some(latest_version);

    // Reorder to Chronological (Oldest -> Newest) for display
    active_updates.reverse();

    // Aggregate considerations
    let mut considerations = String::new();
    for (release, cons) in active_updates {
        // Add Version Header if needed
        if !is_single_latest {
            considerations.push_str(&format!("- **{}**\n", release.tag));
        }
        // Process body lines
        for line in cons.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            // Check if the line already acts as a list item
            let has_bullet = trimmed.starts_with('-') || trimmed.starts_with('*') || trimmed.starts_with('+');

            // Normalize: Ensure the line is a list item
            // If it has a bullet, we keep the raw 'line' to preserve existing nesting/indentation.
            // If it doesn't, we force it to be a bullet item.
            let normalized_line = if has_bullet {
                Cow::Borrowed(line)
            } else {
                Cow::Owned(format!("- {}", trimmed))
            };

            // Indent: Shift everything right if we are nesting under a version header
            if !is_single_latest {
                considerations.push_str("  ");
            }

            considerations.push_str(&normalized_line);
            considerations.push('\n');
        }
    }
    considerations
}

/// Extracts the "Update Considerations" section from the release body, if present.
fn extract_update_considerations(body: &str) -> Option<String> {
    let lines = body.lines();
    let mut capturing = false;
    let mut content = String::new();

    for line in lines {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            if trimmed.to_lowercase().contains("update consideration")
                || trimmed.to_lowercase().contains("update instructions")
                || trimmed.to_lowercase().contains("update guide")
                || trimmed.to_lowercase().contains("upgrade consideration")
                || trimmed.to_lowercase().contains("upgrade instructions")
                || trimmed.to_lowercase().contains("upgrade guide")
                || trimmed.to_lowercase().contains("migration")
            {
                capturing = true;
                continue;
            } else if capturing {
                break;
            }
        }

        if capturing {
            if !content.is_empty() {
                content.push('\n');
            }
            content.push_str(line);
        }
    }

    let trimmed_content = content.trim();
    if trimmed_content.is_empty() {
        None
    } else {
        Some(trimmed_content.to_string())
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;
    use crate::model::IntelliShellRelease;

    #[test]
    fn test_extract_update_considerations() {
        let body = r#"
# Release Notes

Some intro text.

## Update Considerations

This is a critical update.
Please restart your shell.

## Changelog

- Fix bug A
- Add feature B
"#;
        let expected = "This is a critical update.\nPlease restart your shell.";
        assert_eq!(extract_update_considerations(body), Some(expected.to_string()));

        let body_no_considerations = r#"
# Release Notes

## Changelog
- Fix bug A
"#;
        assert_eq!(extract_update_considerations(body_no_considerations), None);

        let body_empty_considerations = r#"
## Update Considerations

## Changelog
"#;
        assert_eq!(extract_update_considerations(body_empty_considerations), None);

        let body_last_section = r#"
## Update Considerations
Last section.
"#;
        assert_eq!(
            extract_update_considerations(body_last_section),
            Some("Last section.".to_string())
        );
    }

    #[test]
    fn test_build_considerations_message() {
        fn make_release(version: &str, body: Option<&str>) -> IntelliShellRelease {
            IntelliShellRelease {
                tag: format!("v{}", version),
                version: Version::parse(version).unwrap(),
                title: "Release".into(),
                body: body.map(|s| s.into()),
                published_at: Utc::now(),
                fetched_at: Utc::now(),
            }
        }

        // Case 1: Multiple updates with considerations
        let releases_multi = vec![
            make_release("1.2.0", Some("## Update Considerations\nCritical 1.2")),
            make_release("1.1.0", Some("No considerations")),
            make_release(
                "1.0.0",
                Some("## Update Considerations\n- Explicit list item\nImplicit item"),
            ),
        ];
        let latest_multi = Version::parse("1.2.0").unwrap();

        let msg_multi = build_considerations_message(&releases_multi, &latest_multi);

        // Expected order: 1.0.0, then 1.2.0
        assert!(msg_multi.contains("- **v1.0.0**"));
        assert!(msg_multi.contains("  - Explicit list item"));
        assert!(msg_multi.contains("  - Implicit item"));
        assert!(msg_multi.contains("- **v1.2.0**"));
        assert!(msg_multi.contains("  - Critical 1.2"));

        // Case 2: Single latest update with considerations
        let releases_single = vec![make_release("1.3.0", Some("## Update Considerations\nJust me"))];
        let latest_single = Version::parse("1.3.0").unwrap();

        let msg_single = build_considerations_message(&releases_single, &latest_single);

        // Should NOT have version header
        assert!(!msg_single.contains("**v1.3.0**"));
        // Should NOT have indentation
        assert!(msg_single.contains("- Just me"));
        assert!(!msg_single.contains("  - Just me"));

        // Case 3: Single OLD update with considerations (e.g. skipped 1.4, installing 1.5 which has none, but 1.4 has)
        // If active_updates has 1 element which is NOT latest, it should have header.
        let releases_gap = vec![
            make_release("1.5.0", None),
            make_release("1.4.0", Some("## Update Considerations\nGap update")),
        ];
        let latest_gap = Version::parse("1.5.0").unwrap();

        let msg_gap = build_considerations_message(&releases_gap, &latest_gap);

        assert!(msg_gap.contains("- **v1.4.0**"));
        assert!(msg_gap.contains("  - Gap update"));
    }
}
