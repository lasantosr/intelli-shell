use std::{fs, io::ErrorKind, process::Command};

use color_eyre::eyre::{Context, Report};
use directories::BaseDirs;

use crate::{
    cli::HistorySource,
    errors::{Result, UserFacingError},
};

/// Reads command history from a specified shell or history manager.
///
/// This function dispatches to a specific reader based on the `source` enum variant.
/// Each reader attempts to find the history in its default location.
pub fn read_history(source: HistorySource) -> Result<String> {
    match source {
        HistorySource::Bash => read_bash_history(),
        HistorySource::Zsh => read_zsh_history(),
        HistorySource::Fish => read_fish_history(),
        HistorySource::Powershell => read_powershell_history(),
        HistorySource::Nushell => read_nushell_history(),
        HistorySource::Atuin => read_atuin_history(),
    }
}

fn read_bash_history() -> Result<String> {
    read_history_from_home(&[".bash_history"])
}

fn read_zsh_history() -> Result<String> {
    read_history_from_home(&[".zsh_history"])
}

fn read_fish_history() -> Result<String> {
    read_history_from_home(&[".local", "share", "fish", "fish_history"])
}

fn read_powershell_history() -> Result<String> {
    let path = if cfg!(windows) {
        vec![
            "AppData",
            "Roaming",
            "Microsoft",
            "Windows",
            "PowerShell",
            "PSReadLine",
            "ConsoleHost_history.txt",
        ]
    } else {
        vec![".local", "share", "powershell", "PSReadLine", "ConsoleHost_history.txt"]
    };
    read_history_from_home(&path)
}

fn read_nushell_history() -> Result<String> {
    // Execute the `nu` command to get a newline-separated list of history entries
    let output = Command::new("nu")
        .arg("-c")
        .arg("history | get command | str join \"\n\"")
        .output();

    match output {
        Ok(output) if output.status.success() => {
            Ok(String::from_utf8(output.stdout).wrap_err("Couldn't read nu output")?)
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stderr.is_empty() {
                tracing::error!("Couldn't execute nu: {stderr}");
            }
            Err(UserFacingError::HistoryNushellFailed.into())
        }
        Err(err) if err.kind() == ErrorKind::NotFound => Err(UserFacingError::HistoryNushellNotFound.into()),
        Err(err) => Err(Report::from(err).wrap_err("Couldn't run nu").into()),
    }
}

fn read_atuin_history() -> Result<String> {
    // Execute the `atuin history list` command
    let output = Command::new("atuin")
        .arg("history")
        .arg("list")
        .arg("--cmd-only")
        .output();

    match output {
        Ok(output) if output.status.success() => {
            Ok(String::from_utf8(output.stdout).wrap_err("Couldn't read atuin output")?)
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stderr.is_empty() {
                tracing::error!("Couldn't execute atuin: {stderr}");
            }
            Err(UserFacingError::HistoryAtuinFailed.into())
        }
        Err(err) if err.kind() == ErrorKind::NotFound => Err(UserFacingError::HistoryAtuinNotFound.into()),
        Err(err) => Err(Report::from(err).wrap_err("Couldn't run atuin").into()),
    }
}

/// A helper function to construct a path from the user's home directory and read the file's content
fn read_history_from_home(path_segments: &[&str]) -> Result<String> {
    let mut path = BaseDirs::new()
        .ok_or(UserFacingError::HistoryHomeDirNotFound)?
        .home_dir()
        .to_path_buf();
    for segment in path_segments {
        path.push(segment);
    }
    fs::read_to_string(&path).map_err(|err| {
        if err.kind() == ErrorKind::NotFound {
            UserFacingError::HistoryFileNotFound(path.to_string_lossy().into_owned()).into()
        } else if err.kind() == ErrorKind::PermissionDenied {
            UserFacingError::FileNotAccessible("read").into()
        } else {
            Report::from(err)
                .wrap_err(format!("Couldn't read history file {}", path.display()))
                .into()
        }
    })
}
