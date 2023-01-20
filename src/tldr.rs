use std::{fs, path::Path};

use anyhow::{bail, Context, Error, Result};
use git2::build::{CheckoutBuilder, RepoBuilder};
use once_cell::sync::Lazy;
use regex::Regex;

use crate::{
    cfg::{cfg_android, cfg_macos, cfg_unix, cfg_windows},
    model::Command,
};

/// Regex to parse tldr pages as stated in [contributing guide](https://github.com/tldr-pages/tldr/blob/main/CONTRIBUTING.md#markdown-format)
static PAGES_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r#"\n\s*- (.+?):?\n\n?\s*`([^`]+)`"#).unwrap());

/// Scrape tldr GitHub: https://github.com/tldr-pages/tldr
pub fn scrape_tldr_github(category: Option<&str>) -> Result<Vec<Command>> {
    scrape_tldr_repo("https://github.com/tldr-pages/tldr.git", category)
}

/// Scrapes any tldr-pages repo that follows the same semantics (maybe a fork?)
pub fn scrape_tldr_repo(url: impl AsRef<str>, category: Option<&str>) -> Result<Vec<Command>> {
    let tmp_dir = tempfile::tempdir()?;
    let repo_path = tmp_dir.path();

    let mut checkout = CheckoutBuilder::default();
    checkout.path("pages/**");

    RepoBuilder::default()
        .with_checkout(checkout)
        .clone(url.as_ref(), repo_path)?;

    let mut result = Vec::new();

    match category {
        Some(category) => {
            if !repo_path.join("pages").join(category).exists() {
                bail!("Category {category} doesn't exist")
            }
            result.append(&mut parse_tldr_folder(
                category,
                repo_path.join("pages").join(category),
            )?);
        }
        None => {
            result.append(&mut parse_tldr_folder(
                "common",
                repo_path.join("pages").join("common"),
            )?);

            cfg_android!(
                result.append(&mut parse_tldr_folder(
                    "android",
                    repo_path.join("pages").join("android"),
                )?);
            );
            cfg_macos!(
                result.append(&mut parse_tldr_folder(
                    "osx",
                    repo_path.join("pages").join("osx"),
                )?);
            );
            cfg_unix!(
                result.append(&mut parse_tldr_folder(
                    "linux",
                    repo_path.join("pages").join("linux"),
                )?);
            );
            cfg_windows!(
                result.append(&mut parse_tldr_folder(
                    "windows",
                    repo_path.join("pages").join("windows"),
                )?);
            );
        }
    }

    Ok(result)
}

/// Parses every file on a tldr-pages folder into [Vec<Command>]
fn parse_tldr_folder(category: impl Into<String>, path: impl AsRef<Path>) -> Result<Vec<Command>> {
    let path = path.as_ref();
    let category = category.into();
    path.read_dir()
        .context("Error reading tldr dir")?
        .map(|r| r.map_err(Error::from))
        .map(|r| r.map(|e| e.path()))
        .map(|r| r.and_then(|p| Ok(fs::read_to_string(p)?)))
        .map(|r| r.map(|r| parse_page(&category, r)))
        .flat_map(|result| match result {
            Ok(vec) => vec.into_iter().map(Ok).collect(),
            Err(er) => vec![Err(er)],
        })
        .collect::<Result<Vec<_>>>()
}

/// Parses a single tldr-page as [Vec<Command>]
fn parse_page(category: impl Into<String>, str: impl AsRef<str>) -> Vec<Command> {
    let category = category.into();
    PAGES_REGEX
        .captures_iter(str.as_ref())
        .map(|c| Command::new(category.clone(), &c[2], &c[1]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_page() -> Result<()> {
        let commands = parse_page(
            "test",
            r#"# git commit

            > Commit files to the repository.
            > More information: <https://git-scm.com/docs/git-commit>.
            
            - Commit staged files to the repository with a message:
            
            `git commit -m "{{message}}"`
            
            - Commit staged files with a message read from a file
            
            `git commit --file {{path/to/commit_message_file}}`
            
            - Auto stage all modified files and commit with a message;
            
            `git commit -a -m "{{message}}"`
            
            - Commit staged files and [S]ign them with the GPG key defined in `~/.gitconfig`
            
            `git commit -S -m "{{message}}"`
            
            - Update the last commit by adding the currently staged changes, changing the commit's hash
            
            `git commit --amend`
            
            - Commit only specific (already staged) files:
            
            `git commit {{path/to/file1}} {{path/to/file2}}`
            
            - Create a commit, even if there are no staged files
            
            `git commit -m "{{message}}" --allow-empty`
        "#,
        );

        assert_eq!(commands.len(), 7);
        assert_eq!(commands.get(0).unwrap().cmd, r#"git commit -m "{{message}}""#);
        assert_eq!(
            commands.get(0).unwrap().description,
            r#"Commit staged files to the repository with a message"#
        );
        assert_eq!(commands.get(3).unwrap().cmd, r#"git commit -S -m "{{message}}""#);
        assert_eq!(
            commands.get(3).unwrap().description,
            r#"Commit staged files and [S]ign them with the GPG key defined in `~/.gitconfig`"#
        );

        Ok(())
    }
}
