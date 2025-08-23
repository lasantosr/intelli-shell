# Troubleshooting

This section addresses common issues you might encounter while using IntelliShell. If you can't find a solution here,
feel free to open an issue on the [GitHub repository](https://github.com/lasantosr/intelli-shell/issues).

## Hotkey and Integration Problems

If hotkeys like <kbd>Ctrl</kbd>+<kbd>Space</kbd> or <kbd>Ctrl</kbd>+<kbd>B</kbd> have no effect, follow these steps:

1. **Restart Your Terminal**: The installation script modifies your shell's profile (e.g., `~/.bashrc`, `~/.zshrc`).
   You must open a new terminal session for these changes to take effect.

2. **Verify Shell Profile**: Ensure the IntelliShell init is evaluated in your shell's profile file. The installer
   should add it automatically, but it's good to check.

3. **Check for Keybinding Issues**: If the steps above are correct, the key combination itself is likely the problem.
   - **Conflicts**: Another application might be intercepting the keys. For example, some desktop environments use
     <kbd>Ctrl</kbd>+<kbd>Space</kbd> to switch keyboard layouts or open a system search.
   - **Terminal Limitations**: Some terminal emulators do not forward all key combinations to the shell. For instance,
     <kbd>Ctrl</kbd>+<kbd>Enter</kbd> (the default "execute" hotkey) is not supported by many terminals.
   - **Solution**: You can change any conflicting or unsupported hotkey. Set the appropriate environment variable in
     your shell profile _before_ the IntelliShell line. See the [Installation Guide](./guide/installation.md#customizing-shell-integration)
     for a full list of integration variables or the [Keybindings Configuration](./configuration/keybindings.md)
     for in-app bindings.

## Installation and Command Issues

### "command not found: intelli-shell"

If your shell cannot find the `intelli-shell` executable after installation:

1. **Restart Your Terminal**: Just like with hotkeys, your shell needs to reload its `PATH` environment variable.

2. **Verify `PATH`**: The installer attempts to add the binary's location to your `PATH`. If this failed, you may
    need to add it manually to your shell's profile file.

### "Permission Denied" errors on Linux/macOS

If you installed manually or are having permission issues, ensure the `intelli-shell` binary is executable.

```sh
# Find the binary and make it executable
chmod +x "$(which intelli-shell)"
```

## General Usage

### How do I edit or delete a bookmarked command?

From the search UI (<kbd>Ctrl</kbd>+<kbd>Space</kbd>), highlight the command you wish to modify.

- **Edit**: Press <kbd>Ctrl</kbd>+<kbd>U</kbd>
- **Delete**: Press <kbd>Ctrl</kbd>+<kbd>D</kbd>

> ⚠️ **Note**: It you can't edit or delete some commands, they might come from the workspace-specific files. You can
> enable user-only search to exclude them or add them you your own user library before updating the alias or description.

### Where is my data stored?

By default, IntelliShell stores its database and configuration in platform-specific user directories. You can override
this by setting the `data_dir` option in your configuration file.

- **Configuration File**:
  - _Linux_: `~/.config/intelli-shell/config.toml`
  - _macOS_: `~/Library/Preferences/org.IntelliShell.Intelli-Shell/config.toml`
  - _Windows_: `%APPDATA%\IntelliShell\Intelli-Shell\config\config.toml`

- **Data Files (Database, Logs)**:
  - _Linux_: `~/.local/share/intelli-shell`
  - _macOS_: `~/Library/Application Support/org.IntelliShell.Intelli-Shell`
  - _Windows_: `%APPDATA%\IntelliShell\Intelli-Shell\data`

### How can I sync commands between machines?

Use the `export` and `import` commands. You can write your library to a file, back it up to a private GitHub Gist, or
even serve it from a local HTTP endpoint. See the [Syncing and Sharing](./guide/syncing.md) guide for examples.

### Imported commands are not appearing

- **Check the Format**: Ensure the file you are importing from follows the correct format. Descriptions should be
  comment lines (`#`) directly preceding the command. Blank lines can act as separators.
- **Use Dry Run**: Use the `--dry-run` flag with the `import` command to preview how IntelliShell will parse the file
  without actually saving anything. This helps diagnose formatting issues.

  ```sh
  intelli-shell import --dry-run /path/to/commands.txt
  ```

### Variable suggestions are not synced

The `import` and `export` commands are designed to manage **commands**, their aliases, and descriptions only. They do
**not** sync the history of values you've used for variables.

If you need to back up and restore your entire IntelliShell state, including variable suggestions, you must manually
copy the database file (`storage.db3`) located in your IntelliShell data directory.

## Advanced Troubleshooting

### Enabling logs

If you encounter a persistent bug, enabling logs can help diagnose the problem. You can do this in two ways:

1. **Configuration File**: In your `config.toml`, set `enabled = true` under the `[logs]` section. You can also adjust
    the log level here.

2. **Environment Variable**: For a quick debug session, set the `INTELLI_LOG` variable. This overrides the config file. For
    example: `INTELLI_LOG=debug intelli-shell search`.

Logs will be written to a file inside the application's data directory.

### Resetting to defaults

If your configuration or database becomes corrupted, you can perform a full reset by deleting the application's data
and configuration directories listed in the "Where is my data stored?" section above.
