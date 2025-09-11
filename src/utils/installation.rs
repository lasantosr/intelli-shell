use std::{env, path::Path};

#[derive(Debug, PartialEq, Eq)]
pub enum InstallationMethod {
    /// Installed via the installer script
    Installer,
    /// Installed via `cargo install`
    Cargo,
    /// Installed via Nix
    Nix,
    /// Likely compiled directly from a source checkout
    Source,
    /// Could not determine the installation method
    Unknown(Option<String>),
}

/// Detects how `intelli-shell` was installed by inspecting the executable's path
pub fn detect_installation_method(data_dir: impl AsRef<Path>) -> InstallationMethod {
    let current_exe = match env::current_exe() {
        Ok(path) => path,
        Err(_) => return InstallationMethod::Unknown(None),
    };

    // Check if the executable is located within the official data directory's `bin` subfolder.
    // This is the strongest indicator of an installation via the script.
    let installer_bin_path = data_dir.as_ref().join("bin");
    if current_exe.starts_with(installer_bin_path) {
        return InstallationMethod::Installer;
    }

    // Fallback to checking for common package manager and development paths
    let path_str = current_exe.to_string_lossy();
    if path_str.starts_with("/nix/store/") {
        // Nix/NixOS path (Linux, macOS)
        InstallationMethod::Nix
    } else if path_str.contains(".cargo/bin") {
        // Cargo path (Linux, macOS, Windows)
        InstallationMethod::Cargo
    } else if path_str.contains("target/debug") || path_str.contains("target/release") {
        // Running from a local build directory
        InstallationMethod::Source
    } else {
        InstallationMethod::Unknown(Some(path_str.to_string()))
    }
}
