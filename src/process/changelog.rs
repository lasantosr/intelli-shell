use color_eyre::Result;
use crossterm::style::{Attribute, Stylize};
use tokio_util::sync::CancellationToken;
use tracing::instrument;

use crate::{
    cli::ChangelogProcess,
    config::Config,
    errors::AppError,
    format_error,
    process::{Process, ProcessOutput},
    service::{CURRENT_VERSION, IntelliShellService},
    utils::{VersionExt, render_markdown_to_ansi},
};

impl Process for ChangelogProcess {
    #[instrument(skip_all)]
    async fn execute(
        self,
        config: Config,
        service: IntelliShellService,
        cancellation_token: CancellationToken,
    ) -> Result<ProcessOutput> {
        let from_tag = self.from.to_tag();

        // Input validation
        if let Some(ref to) = self.to
            && &self.from > to
        {
            return Ok(ProcessOutput::fail().stderr(format_error!(
                config.theme,
                "Invalid criteria: from ({}) > to ({})",
                from_tag.cyan(),
                to.to_tag().cyan()
            )));
        }

        // Retrieve stored releases
        let all_releases = match service.get_or_fetch_releases(false, cancellation_token).await {
            Ok(r) => r,
            Err(AppError::UserFacing(err)) => {
                return Ok(ProcessOutput::fail().stderr(format_error!(config.theme, "{err}")));
            }
            Err(AppError::Unexpected(report)) => return Err(report),
        };

        // Check 'to' version existence
        if let Some(ref to) = self.to
            && !all_releases.iter().any(|r| &r.version >= to)
        {
            return Ok(ProcessOutput::fail().stderr(format_error!(
                config.theme,
                "It looks like {} hasn't been released yet! \nYou can omit the '--to' flag to see all available \
                 releases up to the latest.",
                to.to_tag().red()
            )));
        }

        // Filter from / to
        let filtered_releases = all_releases
            .iter()
            .filter(|r| {
                if r.version < self.from {
                    return false;
                }
                if let Some(ref t) = self.to
                    && &r.version > t
                {
                    return false;
                }
                true
            })
            .collect::<Vec<_>>();

        // Check if any releases were found between the range
        if filtered_releases.is_empty() {
            return Ok(
                ProcessOutput::fail().stderr(format_error!(config.theme, "No releases found matching the criteria"))
            );
        }

        // Filter major / minor
        let filtered_releases = filtered_releases
            .into_iter()
            .filter(|r| {
                if r.version < self.from {
                    return false;
                }
                if let Some(ref t) = self.to
                    && &r.version > t
                {
                    return false;
                }
                if self.major && (r.version.minor != 0 || r.version.patch != 0) {
                    return false;
                }
                if self.minor && r.version.patch != 0 {
                    return false;
                }
                true
            })
            .collect::<Vec<_>>();

        // Check if any releases were found after major/minor filtering
        if filtered_releases.is_empty() {
            let filter_type = match (self.major, self.minor) {
                (true, _) => "major",
                (_, true) => "minor",
                _ => "relevant",
            };

            let msg = match self.to {
                Some(to_ver) => format!(
                    "⚠️ No {} releases found between {} and {}",
                    filter_type,
                    from_tag.cyan(),
                    to_ver.to_tag().cyan()
                ),
                None => format!("⚠️ No {} releases found after {}", filter_type, from_tag.cyan()),
            };

            return Ok(ProcessOutput::success().stderr(msg));
        }

        // Warn on stale 'from' version
        if let Some(oldest_release) = all_releases.last()
            && self.from < oldest_release.version
        {
            eprintln!(
                "⚠️  {} {}",
                config.theme.error.apply(from_tag.red()),
                config
                    .theme
                    .primary
                    .apply("is too old, please check GitHub Releases to view full changelog.")
            );
        } else if !all_releases.iter().any(|r| r.version == self.from) {
            eprintln!(
                "⚠️  {} {}",
                config.theme.error.apply(from_tag.red()),
                config
                    .theme
                    .primary
                    .apply("doesn't exist, but here's the changelog for newer releases.")
            );
        }

        // Prepare changelog and return it
        let changelog = filtered_releases.iter().rev().fold(String::new(), |mut acc, r| {
            let mut title = r.title.as_str();
            let mut body = r.body.as_deref().unwrap_or("").trim_end();

            // If it starts with a level 2 header consider it the release title
            if r.title == r.tag && body.starts_with("## ") {
                if let Some((first_line, rest)) = body.split_once('\n') {
                    title = first_line.trim_start_matches("## ").trim();
                    body = rest.trim();
                } else {
                    title = body.trim_start_matches("## ").trim();
                    body = "";
                }
            }

            let is_current = r.version == *CURRENT_VERSION;
            let is_current_mark = if is_current { " (current)" } else { "" };
            let header = if title != r.tag {
                format!("{}{} - {}", r.tag, is_current_mark, title)
            } else {
                format!("{}{}", r.tag, is_current_mark)
            };

            let line_width = 60usize;
            let line_len = line_width.saturating_sub(header.len() + 4);
            let line = "─".repeat(line_len);

            let mut style = if is_current {
                config.theme.highlight_accent_full()
            } else {
                config.theme.highlight_primary_full()
            };
            style.attributes.set(Attribute::Bold);

            acc.push_str(&style.apply(&format!("── {header} {line}")).to_string());
            acc.push_str("\n\n");

            if !body.is_empty() {
                acc.push_str(&render_markdown_to_ansi(body, &config.theme));
                acc.push_str("\n\n");
            }
            acc
        });

        Ok(ProcessOutput::success().stdout(format!("\n{}\n", changelog.trim_end())))
    }
}
