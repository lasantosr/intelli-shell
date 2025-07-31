use std::{
    env, fs,
    io::{self, IsTerminal, Write},
    panic::AssertUnwindSafe,
    path::Path,
    process,
};

use color_eyre::{Result, eyre::Context};
use intelli_shell::{
    app::App,
    cli::{Cli, CliProcess, Shell},
    config::Config,
    errors, logging,
    process::{OutputInfo, ProcessOutput},
    service::IntelliShellService,
    storage::SqliteStorage,
    utils::{ShellType, get_shell},
};
use tokio::process::Command;

const EXECUTE_PREFIX: &str = "____execute____";
const EXECUTED_OUTPUT: &str = "####EXECUTED####";

const BASH_INIT: &str = include_str!("./_shell/intelli-shell.bash");
const ZSH_INIT: &str = include_str!("./_shell/intelli-shell.zsh");
const FISH_INIT: &str = include_str!("./_shell/intelli-shell.fish");
const POWERSHELL_INIT: &str = include_str!("./_shell/intelli-shell.ps1");

#[tokio::main]
async fn main() -> Result<()> {
    // Read and initialize config
    let config = Config::init(env::var("INTELLI_CONFIG").ok().map(Into::into))?;

    // Initialize logging
    let logs_path = logging::init(&config)?;

    tracing::info!("intelli-shell v{}", env!("CARGO_PKG_VERSION"),);

    // Initialize error handling
    errors::init(
        logs_path,
        AssertUnwindSafe(async move {
            // Parse cli arguments
            let args = Cli::parse_extended();

            // Check for init process before initialization, to avoid unnecessary overhead
            if let CliProcess::Init(init) = args.process {
                tracing::info!("Running 'init' process");
                tracing::debug!("Options: {:?}", init);
                let script = match init.shell {
                    Shell::Bash => BASH_INIT,
                    Shell::Zsh => ZSH_INIT,
                    Shell::Fish => FISH_INIT,
                    Shell::Powershell => POWERSHELL_INIT,
                };
                process_output(
                    OutputInfo {
                        stdout: Some(script.into()),
                        ..Default::default()
                    },
                    args.file_output,
                )?;
                return Ok(());
            }

            // Initialize the storage and the service
            let storage = SqliteStorage::new(&config.data_dir).await?;
            let service = IntelliShellService::new(storage, config.tuning, &config.data_dir, config.check_updates);

            // Run the app
            let output = App::new()?.run(config, service, args.process, args.extra_line).await?;

            // Process the output
            match output {
                ProcessOutput::Execute { cmd } => execute_command(cmd, args.file_output, args.skip_execution).await?,
                ProcessOutput::Output(info) => process_output(info, args.file_output)?,
            }

            Ok(())
        }),
    )
    .await
}

/// Executes the given command
async fn execute_command(command: String, file_output_path: Option<String>, skip_execution: bool) -> Result<()> {
    // If skip_execution is true, we only write the command to the file output path
    // and do not execute it. This is useful for shell integrations that can handle the command
    // execution themselves.
    if skip_execution {
        if let Some(file_output) = file_output_path {
            let fileout = format!("{EXECUTE_PREFIX}{command}");
            tracing::info!("[fileout] {fileout}");
            let path_output = Path::new(&file_output);
            if let Some(parent) = path_output.parent() {
                fs::create_dir_all(parent)
                    .wrap_err_with(|| format!("Failed to create parent directories for: {}", parent.display()))?;
            }
            fs::write(path_output, fileout)
                .wrap_err_with(|| format!("Failed to write to fileout path: {file_output}"))?;
        }
        return Ok(());
    }

    // Let the OS shell parse the command, supporting complex commands, arguments, and pipelines
    let shell = get_shell();
    let shell_arg = match shell {
        ShellType::Cmd => "/c",
        ShellType::WindowsPowerShell => "-Command",
        _ => "-c",
    };

    tracing::info!("Executing command: {shell} {shell_arg} -- {command}");
    let is_file_out = file_output_path.is_some();
    if let Some(file_output) = file_output_path {
        let fileout = EXECUTED_OUTPUT;
        tracing::info!("[fileout] {fileout}");
        let path_output = Path::new(&file_output);
        if let Some(parent) = path_output.parent() {
            fs::create_dir_all(parent)
                .wrap_err_with(|| format!("Failed to create parent directories for: {}", parent.display()))?;
        }
        fs::write(path_output, fileout).wrap_err_with(|| format!("Failed to write to fileout path: {file_output}"))?;
    }

    // Print the command on stderr
    let write_result = if !is_file_out {
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

    // Build the command to execute
    let mut cmd = Command::new(shell.to_string());
    cmd.arg(shell_arg);
    cmd.arg(&command);

    // By default, the child process inherits the parent's stdin, stdout, and stderr
    let status = cmd
        .status()
        .await
        .with_context(|| format!("Failed to execute command: `{command}`"))?;

    // Check if the command was not successful
    if !status.success() {
        if let Some(code) = status.code() {
            process::exit(code);
        } else {
            process::exit(1);
        }
    }

    Ok(())
}

/// Process the output info
fn process_output(info: OutputInfo, file_output_path: Option<String>) -> Result<()> {
    // Determine color usage for stdout and stderr based on env vars and TTY
    let use_color_stderr = should_use_color(io::stderr().is_terminal());
    let use_color_stdout = should_use_color(io::stdout().is_terminal());

    // Write the output, if any
    if let Some(stderr) = info.stderr {
        let stderr_nocolor = strip_ansi_escapes::strip_str(&stderr);
        tracing::info!("[stderr] {stderr_nocolor}");
        let write_result = if use_color_stderr {
            writeln!(io::stderr(), "{stderr}")
        } else {
            writeln!(io::stderr(), "{stderr_nocolor}")
        };
        // Handle broken pipe
        if let Err(err) = write_result {
            if err.kind() != io::ErrorKind::BrokenPipe {
                return Err(err).wrap_err("Failed writing to stderr");
            }
            tracing::error!("Failed writing to stderr: Broken pipe");
        };
    }
    if let Some(file_output) = file_output_path {
        if let Some(fileout) = info.fileout {
            let fileout = strip_ansi_escapes::strip_str(&fileout);
            tracing::info!("[fileout] {fileout}");
            let path_output = Path::new(&file_output);
            if let Some(parent) = path_output.parent() {
                fs::create_dir_all(parent)
                    .wrap_err_with(|| format!("Failed to create parent directories for: {}", parent.display()))?;
            }
            fs::write(path_output, fileout)
                .wrap_err_with(|| format!("Failed to write to fileout path: {file_output}"))?;
        }
    } else if let Some(stdout) = info.stdout {
        let stdout_nocolor = strip_ansi_escapes::strip_str(&stdout);
        tracing::info!("[stdout] {stdout_nocolor}");
        let write_result = if use_color_stdout {
            writeln!(io::stdout(), "{stdout}")
        } else {
            writeln!(io::stdout(), "{stdout_nocolor}")
        };
        // Handle broken pipe
        if let Err(err) = write_result {
            if err.kind() != io::ErrorKind::BrokenPipe {
                return Err(err).wrap_err("Failed writing to stdout");
            }
            tracing::error!("Failed writing to stdout: Broken pipe");
        };
    }

    // Exit with a non-zero status code when the process failed
    if info.failed {
        process::exit(1);
    }

    Ok(())
}

/// Determines whether to use color for a given output stream.
///
/// Precedence:
/// 1. `NO_COLOR` environment variable (if set to any value, disables color)
/// 2. `CLICOLOR_FORCE` environment variable (if set and not "0", forces color)
/// 3. `CLICOLOR` environment variable (if set to "0", disables color)
/// 4. `stream_is_tty` (default if not overridden by env vars)
fn should_use_color(stream_is_tty: bool) -> bool {
    // 1. NO_COLOR environment variable (takes highest precedence)
    if env::var("NO_COLOR").is_ok() {
        return false;
    }

    // 2. CLICOLOR_FORCE environment variable
    if let Ok(force_val) = env::var("CLICOLOR_FORCE") {
        if !force_val.is_empty() && force_val != "0" {
            return true;
        }
    }

    // 3. CLICOLOR environment variable
    if let Ok(clicolor_val) = env::var("CLICOLOR") {
        if clicolor_val == "0" {
            return false;
        }
    }

    // 4. TTY status (default if no strong opinions from env vars)
    stream_is_tty
}
