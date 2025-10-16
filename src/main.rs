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
    cli::{Cli, CliProcess, ConfigProcess, LogsProcess, Shell},
    config::Config,
    errors::{self, AppError},
    format_error, logging,
    process::{OutputInfo, ProcessOutput},
    service::IntelliShellService,
    storage::SqliteStorage,
    utils::execute_shell_command_inherit,
};
use tokio_util::sync::CancellationToken;

// --- Shell Integration Constants ---
const STATUS_DIRTY: &str = "DIRTY\n";
const STATUS_CLEAN: &str = "CLEAN\n";
const ACTION_EXECUTE: &str = "EXECUTE\n";
const ACTION_EXECUTED: &str = "EXECUTED\n";
const ACTION_REPLACE: &str = "REPLACE\n";

// --- Init Script Constants ---
const BASH_INIT: &str = include_str!("./_shell/intelli-shell.bash");
const ZSH_INIT: &str = include_str!("./_shell/intelli-shell.zsh");
const FISH_INIT: &str = include_str!("./_shell/intelli-shell.fish");
const NUSHELL_INIT: &str = include_str!("./_shell/intelli-shell.nu");
const POWERSHELL_INIT: &str = include_str!("./_shell/intelli-shell.ps1");

#[tokio::main]
async fn main() -> Result<()> {
    // Read and initialize config
    let (config, stats) = Config::init(env::var("INTELLI_CONFIG").ok().map(Into::into))?;

    // Prepare logging
    let (logs_path, logs_filter) = logging::resolve_path_and_filter(&config);

    // Initialize error handling
    errors::init(
        logs_filter.is_some().then(|| logs_path.clone()),
        AssertUnwindSafe(async move {
            // Parse cli arguments
            let args = Cli::parse_extended();

            // Create a cancellation token
            let cancellation_token = CancellationToken::new();
            let ctrl_c_token = cancellation_token.clone();

            // Link the cancellation token with the ctrl+c signal
            tokio::spawn(async move {
                tokio::signal::ctrl_c().await.ok();
                ctrl_c_token.cancel();
            });

            // Check for static processes before initialization, to avoid unnecessary overhead
            match args.process {
                CliProcess::Init(init) => {
                    let script = match init.shell {
                        Shell::Bash => BASH_INIT,
                        Shell::Zsh => ZSH_INIT,
                        Shell::Fish => FISH_INIT,
                        Shell::Nushell => NUSHELL_INIT,
                        Shell::Powershell => POWERSHELL_INIT,
                    };
                    let output_info = OutputInfo {
                        stdout: Some(script.into()),
                        ..Default::default()
                    };
                    return handle_output(
                        ProcessOutput::Output(output_info),
                        args.file_output,
                        args.skip_execution,
                        cancellation_token,
                    )
                    .await;
                }
                CliProcess::Config(ConfigProcess { path }) => {
                    if path {
                        println!("{}", stats.config_path.display());
                    } else {
                        edit::edit_file(&stats.config_path)
                            .wrap_err_with(|| format!("Failed to open config file: {}", stats.config_path.display()))?;
                    }
                    return Ok(());
                }
                CliProcess::Logs(LogsProcess { path }) => {
                    if path {
                        println!("{}", logs_path.display());
                    } else {
                        match fs::read_to_string(&logs_path) {
                            Ok(logs_content) if !logs_content.is_empty() => {
                                println!("{logs_content}");
                            }
                            _ => {
                                eprintln!(
                                    "{}",
                                    format_error!(
                                        config.theme,
                                        "No logs found on: {}\n\nMake sure logging is enabled in the config file: {}",
                                        logs_path.display(),
                                        stats.config_path.display()
                                    )
                                )
                            }
                        }
                    }
                    return Ok(());
                }
                _ => (),
            }

            // Initialize logging
            logging::init(logs_path, logs_filter)?;

            // Initial logs
            tracing::info!("intelli-shell v{}", env!("CARGO_PKG_VERSION"));
            match (stats.config_loaded, stats.default_config_path) {
                (true, true) => tracing::info!("Loaded config from default path: {}", stats.config_path.display()),
                (true, false) => tracing::info!("Loaded config from custom path: {}", stats.config_path.display()),
                (false, true) => tracing::info!("No config found at default path: {}", stats.config_path.display()),
                (false, false) => tracing::warn!("No config found at custom path: {}", stats.config_path.display()),
            }
            if stats.default_data_dir {
                tracing::info!("Using default data dir: {}", config.data_dir.display());
            } else {
                tracing::info!("Using custom data dir: {}", config.data_dir.display());
            }

            // Initialize the storage and the service
            let storage = SqliteStorage::new(&config.data_dir)
                .await
                .map_err(AppError::into_report)?;
            let service = IntelliShellService::new(
                storage,
                config.tuning,
                config.ai.clone(),
                &config.data_dir,
                config.check_updates,
            );

            // Run the app
            let app_cancellation_token = cancellation_token.clone();
            let output = App::new(app_cancellation_token)?
                .run(config, service, args.process, args.extra_line)
                .await?;

            // Process the output
            handle_output(output, args.file_output, args.skip_execution, cancellation_token).await
        }),
    )
    .await
}

/// Handles the process output according to the specified options
async fn handle_output(
    output: ProcessOutput,
    file_output_path: Option<String>,
    skip_execution: bool,
    cancellation_token: CancellationToken,
) -> Result<()> {
    // --- Shell Integration ---
    if let Some(path_str) = &file_output_path {
        let mut file_content = String::new();
        match &output {
            ProcessOutput::Execute { cmd } => {
                // When executing a command, the terminal is clean
                file_content.push_str(STATUS_CLEAN);
                if skip_execution {
                    // Shell can execute; tell it to run this command
                    file_content.push_str(ACTION_EXECUTE);
                    file_content.push_str(cmd);
                } else {
                    // Shell cannot execute; intelli-shell ran it
                    file_content.push_str(ACTION_EXECUTED);
                    // No command content is needed
                }
            }
            ProcessOutput::Output(info) => {
                // Determine status based on stderr
                if info.stderr.is_some() {
                    file_content.push_str(STATUS_DIRTY);
                } else {
                    file_content.push_str(STATUS_CLEAN);
                }
                // If there's content for the buffer, add the REPLACE action
                if let Some(cmd) = &info.fileout {
                    file_content.push_str(ACTION_REPLACE);
                    file_content.push_str(cmd);
                }
            }
        }
        // Remove trailing newline to keep the file clean
        let content = file_content.trim_end_matches('\n');

        tracing::info!("[fileout]\n{content}");
        let path_output = Path::new(&path_str);
        if let Some(parent) = path_output.parent() {
            fs::create_dir_all(parent)
                .wrap_err_with(|| format!("Failed to create parent directories for: {}", parent.display()))?;
        }
        fs::write(path_output, content).wrap_err_with(|| format!("Failed to write to fileout path: {path_str}"))?;
    }

    // Handle the output based on its variant
    match output {
        // The process wants to execute a command
        ProcessOutput::Execute { cmd } => {
            // If shell integration is NOT active OR the shell is not capable of executing the command itself
            if !skip_execution {
                // Execute it here
                let status =
                    execute_shell_command_inherit(&cmd, file_output_path.is_none(), cancellation_token).await?;
                // And check if the command failed
                if !status.success() {
                    let code = status.code().unwrap_or(1);
                    tracing::info!("[exit code] {code}");
                    process::exit(code);
                }
            }
        }
        // The process has output to show
        ProcessOutput::Output(info) => {
            // Determine color usage for stdout and stderr based on env vars and TTY
            let use_color_stderr = should_use_color(io::stderr().is_terminal());
            let use_color_stdout = should_use_color(io::stdout().is_terminal());

            // Print stderr if it exists
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
                }
            }

            // Only print to stdout if NOT using file output
            if file_output_path.is_none()
                && let Some(stdout) = info.stdout
            {
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
                }
            }

            // Exit with a non-zero status code when the process failed
            if info.failed {
                tracing::info!("[exit code] 1");
                process::exit(1);
            }
        }
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
    if let Ok(force_val) = env::var("CLICOLOR_FORCE")
        && !force_val.is_empty()
        && force_val != "0"
    {
        return true;
    }

    // 3. CLICOLOR environment variable
    if let Ok(clicolor_val) = env::var("CLICOLOR")
        && clicolor_val == "0"
    {
        return false;
    }

    // 4. TTY status (default if no strong opinions from env vars)
    stream_is_tty
}
