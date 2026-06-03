use std::{env, path::Path};

#[derive(Debug, PartialEq, Eq)]
pub enum InstallationMethod {
    /// Installed via the installer script
    Installer,
    /// Installed via `cargo install`
    Cargo,
    /// Installed via Nix
    Nix,
    /// Installed via Homebrew
    Homebrew,
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
    detect_installation_method_inner(&current_exe, data_dir.as_ref())
}

fn detect_installation_method_inner(current_exe: &Path, data_dir: &Path) -> InstallationMethod {
    // Check if the executable is located within the official data directory's `bin` subfolder.
    // This is the strongest indicator of an installation via the script.
    let installer_bin_path = data_dir.join("bin");
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
    } else if path_str.starts_with("/opt/homebrew/")
        || path_str.starts_with("/home/linuxbrew/.linuxbrew/")
        || path_str.contains("/Cellar/")
        || path_str.contains("/homebrew/")
    {
        // Homebrew installation path
        InstallationMethod::Homebrew
    } else {
        InstallationMethod::Unknown(Some(path_str.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_installation_method() {
        let data_dir = Path::new("/home/user/.config/intelli-shell");

        // Test installer path
        assert_eq!(
            detect_installation_method_inner(
                Path::new("/home/user/.config/intelli-shell/bin/intelli-shell"),
                data_dir
            ),
            InstallationMethod::Installer
        );

        // Test Nix path
        assert_eq!(
            detect_installation_method_inner(
                Path::new("/nix/store/abc-intelli-shell/bin/intelli-shell"),
                data_dir
            ),
            InstallationMethod::Nix
        );

        // Test Cargo path
        assert_eq!(
            detect_installation_method_inner(
                Path::new("/home/user/.cargo/bin/intelli-shell"),
                data_dir
            ),
            InstallationMethod::Cargo
        );

        // Test Source path
        assert_eq!(
            detect_installation_method_inner(
                Path::new("/home/user/projects/intelli-shell/target/release/intelli-shell"),
                data_dir
            ),
            InstallationMethod::Source
        );

        // Test Homebrew path (Apple Silicon)
        assert_eq!(
            detect_installation_method_inner(
                Path::new("/opt/homebrew/bin/intelli-shell"),
                data_dir
            ),
            InstallationMethod::Homebrew
        );

        // Test Homebrew path (Intel Mac / Cellar)
        assert_eq!(
            detect_installation_method_inner(
                Path::new("/usr/local/Cellar/intelli-shell/3.4.3/bin/intelli-shell"),
                data_dir
            ),
            InstallationMethod::Homebrew
        );

        // Test Linuxbrew path
        assert_eq!(
            detect_installation_method_inner(
                Path::new("/home/linuxbrew/.linuxbrew/bin/intelli-shell"),
                data_dir
            ),
            InstallationMethod::Homebrew
        );

        // Test Unknown path
        assert_eq!(
            detect_installation_method_inner(
                Path::new("/usr/local/bin/intelli-shell"),
                data_dir
            ),
            InstallationMethod::Unknown(Some("/usr/local/bin/intelli-shell".to_string()))
        );
    }
}
