# Installation

Welcome to the IntelliShell installation guide. Getting IntelliShell running on your system involves two main steps:

1. **Install the Binary**: Place the `intelli-shell` executable on your machine so you can run it.
2. **Configure Shell Integration**: Update your shell's profile (e.g., `~/.bashrc`, `~/.zshrc`) to enable the
    interactive keybindings (`ctrl+space`, etc.).

This guide covers the recommended automatic installation and alternative manual methods.

## Method 1: Automatic Installation (Recommended)

This is the fastest and easiest way to get started. The installer script automatically detects your OS and architecture,
downloads the correct binary, and sets up the shell integration for you.

> **Note**: The installer script will attempt to update all detected shell profiles.
> If you prefer to handle shell configuration manually, you can set the `INTELLI_SKIP_PROFILE=1` environment variable
> before running the script.

### Linux & macOS

Run the following command in your terminal:

```sh
curl -sSf https://raw.githubusercontent.com/lasantosr/intelli-shell/main/install.sh | sh
```

### Windows

> **Prerequisites**:
>
> - The [Microsoft Visual C++ Redistributable](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist)
> is required.

- **For PowerShell users**: You may need to allow script execution first and then run the installer:

  ```powershell
  Set-ExecutionPolicy RemoteSigned -Scope CurrentUser
  irm https://raw.githubusercontent.com/lasantosr/intelli-shell/main/install.ps1 | iex
  ```

- **For POSIX-like shell users (Git Bash, WSL, etc.)**: Same script as for Linux & macOS:

  ```sh
  curl -sSf https://raw.githubusercontent.com/lasantosr/intelli-shell/main/install.sh | sh
  ```

## Method 2: Manual Installation

These methods are for users who prefer more control or cannot run remote scripts. If you install manually, you **must**
also configure your shell integration to enable the hotkeys (see the "Updating Profile Files" section below).

### Option A: From Source

If you have the Rust toolchain installed, you can build and install the binary directly from _crates.io_:

```sh
LIBSQLITE3_FLAGS="-DSQLITE_ENABLE_MATH_FUNCTIONS" cargo install intelli-shell --locked
```

### Option B: From Pre-compiled Binaries

You can also install pre-built binaries without compiling them from source.

- **With `cargo-binstall`**:

  ```sh
  cargo binstall intelli-shell --locked
  ```

- **From GitHub Releases**:
  1. Go to the [**latest release page**](https://github.com/lasantosr/intelli-shell/releases/latest).
  2. Download the archive (`.tar.gz` or `.zip`) for your operating system and architecture.
  3. Extract the `intelli-shell` executable.
  4. Move it to a directory included in your system's `PATH` (e.g., `/usr/local/bin` or `~/.local/bin`).

## Updating Profile Files

> **Important**: This step is handled automatically by the installer script. You only need to do this if you installed
> IntelliShell manually (e.g., with Cargo) or if you used the `INTELLI_SKIP_PROFILE=1` option with the script.

To enable the interactive hotkeys, you must add a line to your shell's configuration file that sources the IntelliShell
initialization script.

<details>
  <summary>Click here for shell-specific instructions</summary>

- **Bash**: Add to `~/.bashrc` or `~/.bash_profile`

  ```bash
  eval "$(intelli-shell init bash)"
  ```

- **Zsh**: Add to `~/.zshrc`

  ```zsh
  eval "$(intelli-shell init zsh)"
  ```

- **Fish**: Add to `~/.config/fish/config.fish`

  ```fish
  intelli-shell init fish | source
  ```

- **Nushell**: Add to your Nushell config file (find it with `$nu.config-path`)

  ```nu
  mkdir ($nu.data-dir | path join "vendor/autoload")
  intelli-shell init nushell | save -f ($nu.data-dir | path join "vendor/autoload/intelli-shell.nu")
  ```

- **PowerShell**: Add to your profile (find it with `$Profile`)

  ```powershell
  intelli-shell init powershell | Out-String | Invoke-Expression
  ```

</details>

## Customizing Keybindings

You can override the default keybindings by setting environment variables in your shell's profile file **before** the
line that sources the IntelliShell init command.

- `INTELLI_SEARCH_HOTKEY`: Overrides the default `ctrl+space` hotkey for searching commands.
- `INTELLI_BOOKMARK_HOTKEY`: Overrides the default `ctrl+b` hotkey to bookmark a command.
- `INTELLI_VARIABLE_HOTKEY`: Overrides the default `ctrl+l` hotkey for replacing variables.
- `INTELLI_FIX_HOTKEY`: Overrides the default `ctrl+x` hotkey for fixing commands.
- `INTELLI_SKIP_ESC_BIND=1`: Prevents IntelliShell from binding the `esc` key to clear the current line in the terminal.

> For keybinding syntax, refer to your shell's documentation (`bindkey` for Zsh, `bind` for Bash). For example, to
> change the search hotkey in Bash, add `export INTELLI_SEARCH_HOTKEY=\\C-t` to your `.bashrc`.

## Verify Your Installation

After installing and configuring your shell, **open a new terminal session** to ensure the changes are loaded. You can
verify that the binary is working by running:

```sh
intelli-shell --version
```

If the command is found and the hotkeys work, you are ready to go!

---

Let's move on to [**Key Concepts**](./key_concepts.md).
