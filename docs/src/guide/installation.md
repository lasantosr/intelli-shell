# Installation

Welcome to the IntelliShell installation guide. Getting IntelliShell running on your system involves two main steps:

1. **Download the Binary**: This places the `intelli-shell` executable on your machine.
2. **Configure Shell Integration**: This involves updating your shell's profile (e.g., `~/.bashrc`, `~/.zshrc`) to
   source the init output that enables the interactive keybindings (`ctrl+space`, etc.).

The easiest way to perform both steps is with the official installer script.

## Method 1: Installer Script

This is the fastest way to get started. The script automatically handles downloading the correct binary for your system
and setting up the shell integration.

If you don't want the script to update the profile files, you can set `INTELLI_SKIP_PROFILE=1` environment variable
before installing.

### Linux, macOS & Windows on sh-compatible shell (Bash, Zsh, Fish, Nu, Git Bash)

```sh
curl -sSf https://raw.githubusercontent.com/lasantosr/intelli-shell/main/install.sh | sh
```

After installing it on any shell, it should work in all of them.

### Windows (PowerShell)

```powershell
irm https://raw.githubusercontent.com/lasantosr/intelli-shell/main/install.ps1 | iex
```

After installing it with powershell, it should also work on cmd (without hotkeys).

> The Microsoft Visual C++ Redistributable is required. You can download it from
> [Microsoft's official site](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist).
> You may also need to run `Set-ExecutionPolicy RemoteSigned -Scope CurrentUser` if you haven't run remote scripts before.

## Method 2: Building from Source

If you have the Rust toolchain installed, you can build and install the binary directly from _crates.io_.

```sh
LIBSQLITE3_FLAGS="-DSQLITE_ENABLE_MATH_FUNCTIONS" cargo install intelli-shell --locked
```

To enable hotkeys integration, additional steps are required:

<details>
  <summary>Details</summary>

Edit your profile to source the init output:

- Bash: `~/.bashrc` or `~/.bash_profile`

  ```bash
  eval "$(intelli-shell init bash)"
  ```

- Zsh: `~/.zshrc`

  ```zsh
  eval "$(intelli-shell init zsh)"
  ```

- Fish: `~/.config/fish/config.fish`

  ```fish
  intelli-shell init fish | source
  ```

- Nushell: `~/.config/nushell/config.nu`

  ```nu
  mkdir ($nu.data-dir | path join "vendor/autoload")
  intelli-shell init nushell | save -f ($nu.data-dir | path join "vendor/autoload/intelli-shell.nu")
  ```

- Powershell: `$Profile`

  ```pwsh
  intelli-shell init powershell | Out-String | Invoke-Expression
  ```

</details>

## Customizing Shell Integration

These variables customize the keybindings to integrate with IntehliShell. They should be set in your shell's profile
file (e.g., `~/.bashrc`, `~/.zshrc`) **before** the line that sources the IntelliShell init command.

- `INTELLI_SEARCH_HOTKEY`: Overrides the default `ctrl+space` hotkey for searching commands.
- `INTELLI_BOOKMARK_HOTKEY`: Overrides the default `ctrl+b` hotkey to bookmark a command.
- `INTELLI_VARIABLE_HOTKEY`: Overrides the default `ctrl+l` hotkey for replacing variables.
- `INTELLI_FIX_HOTKEY`: Overrides the default `ctrl+x` hotkey for fixing commands.
- `INTELLI_SKIP_ESC_BIND=1`: Prevents IntelliShell from binding the `esc` key to clear the current line in the terminal.

> For keybinding syntax, refer to your shell's documentation (`bindkey` for Zsh, `bind` for Bash). For example, to
> change the search hotkey in Bash, you would add the line `export INTELLI_SEARCH_HOTKEY=\\C-t` to your `.bashrc`.

## Verify Your Installation

After installing, open a new terminal session to ensure the changes are loaded. You can verify that the `intelli-shell`
binary is working by running:

```sh
intelli-shell --version
```

---

If the command is found and you can use shortcuts (if configured), you are ready to go!
Let's move on to [**Basic Usage**](./basic_usage.md).
