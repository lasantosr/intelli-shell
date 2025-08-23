use std::{
    cmp::Ordering,
    collections::BTreeMap,
    env,
    ffi::OsStr,
    io::{self, Read, Write},
    ops::Deref,
    path::{Path, PathBuf},
    process::{self, ExitStatus, Stdio},
    sync::LazyLock,
    time::Duration,
};

use color_eyre::eyre::Context;
use ignore::WalkBuilder;
use os_info::Info;
use sysinfo::{Pid, System};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    signal,
};
use wait_timeout::ChildExt;

#[derive(Debug)]
pub struct ShellInfo {
    pub kind: ShellType,
    pub version: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, strum::Display, strum::EnumString)]
pub enum ShellType {
    #[strum(serialize = "cmd", serialize = "cmd.exe")]
    Cmd,
    #[strum(serialize = "powershell", serialize = "powershell.exe")]
    WindowsPowerShell,
    #[strum(serialize = "pwsh", serialize = "pwsh.exe")]
    PowerShellCore,
    #[strum(to_string = "bash", serialize = "bash.exe")]
    Bash,
    #[strum(serialize = "sh")]
    Sh,
    #[strum(serialize = "fish")]
    Fish,
    #[strum(serialize = "zsh")]
    Zsh,
    #[strum(default, to_string = "{0}")]
    Other(String),
}

static PARENT_SHELL_INFO: LazyLock<ShellInfo> = LazyLock::new(|| {
    let pid = Pid::from_u32(process::id());

    tracing::debug!("Retrieving info for pid {pid}");
    let sys = System::new_all();

    let parent_process = sys
        .process(Pid::from_u32(process::id()))
        .expect("Couldn't retrieve current process from pid")
        .parent()
        .and_then(|parent_pid| sys.process(parent_pid));

    let Some(parent) = parent_process else {
        let default = if cfg!(target_os = "windows") {
            ShellType::WindowsPowerShell
        } else {
            ShellType::Sh
        };
        tracing::warn!("Couldn't detect shell, assuming {default}");
        return ShellInfo {
            kind: default,
            version: None,
        };
    };

    let parent_name = parent
        .name()
        .to_str()
        .expect("Invalid parent shell name")
        .trim()
        .to_lowercase();

    let kind = ShellType::try_from(parent_name.as_str()).expect("infallible");
    tracing::info!("Detected shell: {kind}");

    let exe_path = parent
        .exe()
        .map(|p| p.as_os_str())
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| parent_name.as_ref());
    let version = get_shell_version(&kind, exe_path).inspect(|v| tracing::info!("Detected shell version: {v}"));

    ShellInfo { kind, version }
});

/// A helper function to get the version from a shell's executable path
fn get_shell_version(shell_kind: &ShellType, shell_path: impl AsRef<OsStr>) -> Option<String> {
    // `cmd.exe` version is tied to the OS version, so we don't query it
    if *shell_kind == ShellType::Cmd {
        return None;
    }

    // Most shells respond to `--version`, except PowerShell
    let mut command = std::process::Command::new(shell_path);
    if matches!(shell_kind, ShellType::PowerShellCore | ShellType::WindowsPowerShell) {
        command.args([
            "-Command",
            "'PowerShell {0} ({1} Edition)' -f $PSVersionTable.PSVersion, $PSVersionTable.PSEdition",
        ]);
    } else {
        command.arg("--version");
    }

    // Configure pipes for stdout and stderr to capture the output manually
    let mut child = match command.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn() {
        Ok(child) => child,
        Err(err) => {
            tracing::warn!("Failed to spawn shell process: {err}");
            return None;
        }
    };

    // Wait for the process to exit, with a timeout
    match child.wait_timeout(Duration::from_millis(250)) {
        // The command finished within the timeout period
        Ok(Some(status)) => {
            if status.success() {
                let mut output = String::new();
                // Read the output from the stdout pipe
                if let Some(mut stdout) = child.stdout {
                    stdout.read_to_string(&mut output).unwrap_or_default();
                }
                // Return just the first line of the output
                Some(output.lines().next().unwrap_or("").trim().to_string()).filter(|v| !v.is_empty())
            } else {
                tracing::warn!("Shell version command failed with status: {}", status);
                None
            }
        }
        // The command timed out
        Ok(None) => {
            // Kill the child process to prevent it from running forever
            if let Err(err) = child.kill() {
                tracing::warn!("Failed to kill timed-out process: {err}");
            }
            tracing::warn!("Shell version command timed out");
            None
        }
        // An error occurred while waiting
        Err(err) => {
            tracing::warn!("Error waiting for shell version command: {err}");
            None
        }
    }
}

/// Retrieves information about the current shell, including its type and version
pub fn get_shell_info() -> &'static ShellInfo {
    PARENT_SHELL_INFO.deref()
}

/// Retrieves the current shell type
pub fn get_shell_type() -> &'static ShellType {
    &get_shell_info().kind
}

/// A helper function to get the version from an executable (e.g. git)
pub fn get_executable_version(root_cmd: impl AsRef<OsStr>) -> Option<String> {
    if root_cmd.as_ref().is_empty() {
        return None;
    }

    // Most shells commands respond to `--version`
    let mut child = std::process::Command::new(root_cmd)
        .arg("--version")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;

    // Wait for the process to exit, with a timeout
    match child.wait_timeout(Duration::from_millis(250)) {
        Ok(Some(status)) if status.success() => {
            let mut output = String::new();
            if let Some(mut stdout) = child.stdout {
                stdout.read_to_string(&mut output).unwrap_or_default();
            }
            Some(output.lines().next().unwrap_or("").trim().to_string()).filter(|v| !v.is_empty())
        }
        Ok(None) => {
            if let Err(err) = child.kill() {
                tracing::warn!("Failed to kill timed-out process: {err}");
            }
            None
        }
        _ => None,
    }
}

static OS_INFO: LazyLock<Info> = LazyLock::new(|| {
    let info = os_info::get();
    tracing::info!("Detected OS: {info}");
    info
});

/// Retrieves the operating system information
pub fn get_os_info() -> &'static Info {
    &OS_INFO
}

static WORING_DIR: LazyLock<String> = LazyLock::new(|| {
    std::env::current_dir()
        .inspect_err(|err| tracing::warn!("Couldn't retrieve current dir: {err}"))
        .ok()
        .and_then(|p| p.to_str().map(|s| s.to_owned()))
        .unwrap_or_default()
});

/// Retrieves the working directory
pub fn get_working_dir() -> &'static str {
    WORING_DIR.deref()
}

/// Formats an env var name into its shell representation, based on the current shell
pub fn format_env_var(var: impl AsRef<str>) -> String {
    let var = var.as_ref();
    match get_shell_type() {
        ShellType::Cmd => format!("%{var}%"),
        ShellType::WindowsPowerShell | ShellType::PowerShellCore => format!("$env:{var}"),
        _ => format!("${var}"),
    }
}

/// Generates a string representation of the current working directory tree, respecting .gitignore files
pub fn generate_working_dir_tree(max_depth: usize, entry_limit: usize) -> Option<String> {
    let root = PathBuf::from(get_working_dir());
    if !root.is_dir() {
        return None;
    }

    let root_canon = root.canonicalize().ok()?;

    // Phase 1: Collect all entries by depth and also get total child counts for every directory
    let mut entries_by_depth: BTreeMap<usize, Vec<ignore::DirEntry>> = BTreeMap::new();
    let mut total_child_counts: BTreeMap<PathBuf, usize> = BTreeMap::new();
    let walker = WalkBuilder::new(&root_canon).max_depth(Some(max_depth + 1)).build();

    for entry in walker.flatten() {
        if entry.depth() == 0 {
            continue;
        }
        if let Some(parent_path) = entry.path().parent() {
            *total_child_counts.entry(parent_path.to_path_buf()).or_default() += 1;
        }
        entries_by_depth.entry(entry.depth()).or_default().push(entry);
    }

    // Phase 2: Create a limited list of entries using the breadth-first approach
    let mut limited_entries: Vec<ignore::DirEntry> = Vec::with_capacity(entry_limit);
    'outer: for (_depth, entries) in entries_by_depth {
        for entry in entries {
            if limited_entries.len() >= entry_limit {
                break 'outer;
            }
            limited_entries.push(entry);
        }
    }

    // Phase 3: Populate the display tree and add "..." where contents are truncated
    let mut dir_children: BTreeMap<PathBuf, Vec<(String, bool)>> = BTreeMap::new();
    for entry in limited_entries {
        let is_dir = entry.path().is_dir();
        if let Some(parent_path) = entry.path().parent() {
            let file_name = entry.file_name().to_string_lossy().to_string();
            dir_children
                .entry(parent_path.to_path_buf())
                .or_default()
                .push((file_name, is_dir));
        }
    }
    for (path, total_count) in total_child_counts {
        let displayed_count = dir_children.get(&path).map_or(0, |v| v.len());
        if displayed_count < total_count {
            dir_children.entry(path).or_default().push(("...".to_string(), false));
        }
    }

    // Sort the children in each directory alphabetically for consistent output
    for children in dir_children.values_mut() {
        children.sort_by(|a, b| {
            // "..." is always last
            if a.0 == "..." {
                Ordering::Greater
            } else if b.0 == "..." {
                Ordering::Less
            } else {
                // Otherwise, sort alphabetically
                a.0.cmp(&b.0)
            }
        });
    }

    // Phase 4: Build the final string
    let mut tree_string = format!("{} (current working dir)\n", root_canon.display());
    build_tree_from_map(&root_canon, "", &mut tree_string, &dir_children);
    Some(tree_string)
}

/// Recursively builds the tree string from the pre-compiled map of directory children
fn build_tree_from_map(
    dir_path: &Path,
    prefix: &str,
    output: &mut String,
    dir_children: &BTreeMap<PathBuf, Vec<(String, bool)>>,
) {
    let Some(entries) = dir_children.get(dir_path) else {
        return;
    };

    let mut iter = entries.iter().peekable();
    while let Some((name, is_dir)) = iter.next() {
        let is_last = iter.peek().is_none();
        let connector = if is_last { "└── " } else { "├── " };
        let new_prefix = format!("{prefix}{}", if is_last { "    " } else { "│   " });

        if *is_dir {
            // This is a directory; let's see if we can collapse it
            let mut path_components = vec![name.clone()];
            let mut current_path = dir_path.join(name);

            // Keep collapsing as long as the current directory has only one child, which is also a directory
            while let Some(children) = dir_children.get(&current_path) {
                if children.len() == 1 {
                    let (child_name, child_is_dir) = &children[0];
                    if *child_is_dir {
                        path_components.push(child_name.clone());
                        current_path.push(child_name);
                        // Continue to the next level of nesting
                        continue;
                    }
                }
                // Stop collapsing
                break;
            }

            // Print the combined, collapsed path.
            let collapsed_name = path_components.join("/");
            output.push_str(&format!("{prefix}{connector}{collapsed_name}/\n"));

            // Recurse using the final path in the chain
            build_tree_from_map(&current_path, &new_prefix, output, dir_children);
        } else {
            // This is a file or "...", print it normally.
            output.push_str(&format!("{prefix}{connector}{name}\n"));
        }
    }
}

/// Executes a shell command, inheriting the parent's `stdout` and `stderr`
pub async fn execute_shell_command_inherit(command: &str, include_prompt: bool) -> color_eyre::Result<ExitStatus> {
    let mut cmd = prepare_command_execution(command, include_prompt)?;

    // Spawn the child process to get a handle to it
    let mut child = cmd
        .spawn()
        .with_context(|| format!("Failed to spawn command: `{command}`"))?;

    // Race the child process against a Ctrl+C signal
    let status = tokio::select! {
        // Prioritize Ctrl+C handler
        biased;
        // User presses Ctrl+C
        _ = signal::ctrl_c() => {
            tracing::info!("Received Ctrl+C, terminating child process...");
            // Send a kill signal to the child process
            child.kill().await.with_context(|| format!("Failed to kill child process for command: `{command}`"))?;
            // Wait for the process to exit and get its status
            child.wait().await.with_context(|| "Failed to await child process after kill")?
        }
        // The child process completes on its own
        status = child.wait() => {
            status.with_context(|| format!("Child process for command `{command}` failed"))?
        }
    };

    Ok(status)
}

/// Executes a shell command, capturing `stdout` and `stderr`.
///
/// While capturing, it simultaneously prints both streams to the parent's `stderr` in real-time.
pub async fn execute_shell_command_capture(
    command: &str,
    include_prompt: bool,
) -> color_eyre::Result<(ExitStatus, String, bool)> {
    let mut cmd = prepare_command_execution(command, include_prompt)?;

    // Configure the command to capture output streams by creating pipes
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .with_context(|| format!("Failed to spawn command: `{command}`"))?;

    // Create buffered readers for the child's output streams
    let mut stdout_reader = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut stderr_reader = BufReader::new(child.stderr.take().unwrap()).lines();

    let mut output_capture = String::new();

    // Flag to track if the process was terminated by our signal handler
    let mut terminated_by_signal = false;

    // Use boolean flags to track when each stream is finished
    let mut stdout_done = false;
    let mut stderr_done = false;

    // Loop until both stdout and stderr streams have been completely read
    loop {
        tokio::select! {
            // Prioritize Ctrl+C handler
            biased;
            // User presses Ctrl+C
            _ = signal::ctrl_c() => {
                tracing::info!("Received Ctrl+C, terminating child process...");
                // Kill the child process, this will also cause the stdout/stderr streams to close
                child.kill().await.with_context(|| format!("Failed to kill child process for command: `{command}`"))?;
                // Set the flag to true since we handled the signal
                terminated_by_signal = true;
                // Break the loop to proceed to the final `child.wait()`
                break;
            },
            // Read from stdout if it's not done yet
            res = stdout_reader.next_line(), if !stdout_done => {
                match res {
                    Ok(Some(line)) => {
                        writeln!(io::stderr(), "{line}")?;
                        output_capture.push_str(&line);
                        output_capture.push('\n');
                    },
                    _ => stdout_done = true,
                }
            },
            // Read from stderr if it's not done yet
            res = stderr_reader.next_line(), if !stderr_done => {
                match res {
                    Ok(Some(line)) => {
                        writeln!(io::stderr(), "{line}")?;
                        output_capture.push_str(&line);
                        output_capture.push('\n');
                    },
                    _ => stderr_done = true,
                }
            },
            // This branch is taken once both output streams are done
            else => break,
        }
    }

    // Wait for the process to fully exit to get its final status
    let status = child.wait().await.wrap_err("Failed to wait for command")?;

    Ok((status, output_capture, terminated_by_signal))
}

/// Builds a base `Command` object for executing a command string via the OS shell
fn prepare_command_execution(command: &str, include_prompt: bool) -> color_eyre::Result<tokio::process::Command> {
    // Let the OS shell parse the command, supporting complex commands, arguments, and pipelines
    let shell = get_shell_type();
    let shell_arg = match shell {
        ShellType::Cmd => "/c",
        ShellType::WindowsPowerShell => "-Command",
        _ => "-c",
    };

    tracing::info!("Executing command: {shell} {shell_arg} -- {command}");

    // Print the command on stderr
    let write_result = if include_prompt {
        writeln!(
            io::stderr(),
            "{}{command}",
            env::var("INTELLI_EXEC_PROMPT").as_deref().unwrap_or("> "),
        )
    } else {
        writeln!(io::stderr(), "{command}")
    };
    // Handle broken pipe
    if let Err(err) = write_result {
        if err.kind() != io::ErrorKind::BrokenPipe {
            return Err(err).wrap_err("Failed writing to stderr");
        }
        tracing::error!("Failed writing to stderr: Broken pipe");
    };

    // Build the base command object
    let mut cmd = tokio::process::Command::new(shell.to_string());
    cmd.arg(shell_arg).arg(command).kill_on_drop(true);
    Ok(cmd)
}
