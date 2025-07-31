use std::{ops::Deref, process, sync::LazyLock};

use sysinfo::{Pid, System};

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

static PARENT_SHELL: LazyLock<ShellType> = LazyLock::new(|| {
    let parent_name = {
        let pid = Pid::from_u32(process::id());

        tracing::debug!("Retrieving info for pid {pid}");
        let sys = System::new_all();

        sys.process(Pid::from_u32(process::id()))
            .expect("Couldn't retrieve current process from pid")
            .parent()
            .and_then(|parent_pid| sys.process(parent_pid))
            .and_then(|parent_process| parent_process.name().to_str().map(|s| s.trim().to_lowercase()))
    };

    ShellType::try_from(
        parent_name
            .as_deref()
            .inspect(|shell| tracing::info!("Detected shell: {shell}"))
            .unwrap_or_else(|| {
                let default = if cfg!(target_os = "windows") {
                    "powershell"
                } else {
                    "sh"
                };
                tracing::warn!("Couldn't detect shell, assuming {default}");
                default
            }),
    )
    .expect("infallible")
});

/// Retrieves the current shell
pub fn get_shell() -> &'static ShellType {
    PARENT_SHELL.deref()
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
    match get_shell() {
        ShellType::Cmd => format!("%{var}%"),
        ShellType::WindowsPowerShell | ShellType::PowerShellCore => format!("$env:{var}"),
        _ => format!("${var}"),
    }
}
