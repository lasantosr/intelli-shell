# General Settings

This section covers the general configuration options that control the core behavior and appearance of IntelliShell,
such as the user interface mode and the location where application data is stored. These settings are located at the
top level of your `config.toml` file.

## Data Directory

The `data_dir` setting specifies the directory where IntelliShell stores its data, including the command database
(`storage.db3`), logs, and other state files.

If you leave this setting as an empty string (`""`), IntelliShell will use the default system-specific data directory:

- **Linux/macOS**: `~/.local/share/intelli-shell` (or `$XDG_DATA_HOME/intelli-shell` if set)
- **Windows**: `%APPDATA%\IntelliShell\Intelli-Shell\data`

You can specify a custom path to store your data in a different location, such as a synced cloud folder.

```toml
{{#include ../../../default_config.toml:15:19}}
```

### Overriding with `INTELLI_STORAGE`

For more direct control, especially in containerized or portable environments, you can use the `INTELLI_STORAGE`
environment variable. If set, this variable must contain the **full path** to your database file, not just a
directory.

This variable takes precedence over the `data_dir` setting just for the database, and IntelliShell will use the specified
file for all database operations.

```sh
# Example for Linux/macOS
export INTELLI_STORAGE="/path/to/my/custom.db3"
```

## Update Checks

This setting controls whether IntelliShell automatically checks for new versions upon startup. When enabled, it helps
ensure you are always using the latest version with the newest features and bug fixes.

- **`check_updates = true`** (Default): The application will check for updates at startup. If a new version is
  available, a notification will be shown in the TUI.
- **`check_updates = false`**: Disables the automatic update check.

```toml
{{#include ../../../default_config.toml:21:22}}
```

## UI Rendering Mode

The `inline` setting controls how the interactive Terminal User Interface (TUI) is displayed. You can choose between
a compact, inline view or an immersive, full-screen experience.

- **`inline = true`** (Default): The TUI renders directly below your current shell prompt. This mode is less
    intrusive and allows you to see your previous commands while you search.

- **`inline = false`**: The TUI takes over the entire terminal window, providing a more focused, full-screen
    experience.

```toml
{{#include ../../../default_config.toml:24:27}}
```

## Gist Integration

The `[gist]` section allows you to configure default settings for importing from and exporting to GitHub Gists.
This is useful if you frequently use the same Gist to back up or share your commands.

- **`id`**: The ID of your default GitHub Gist. You can find this in the Gist's URL.
- **`token`**: A GitHub Personal Access Token with `gist` scope. This is required for creating or updating private
    Gists.

```toml
{{#include ../../../default_config.toml:33:39}}
```

## Search

The `[search]` section lets you fine-tune the default behavior of the interactive search.

- **`delay`**: The time in milliseconds that IntelliShell waits after you stop typing before it starts searching
- **`mode`**: The default search algorithm, can be `auto`, `fuzzy`, `regex`, `exact`, or `relaxed`
- **`user_only`**: If set to `true`, searches will exclude commands from `tldr` and `.intellishell` file

These settings can be toggled on-the-fly within the search UI using the default keybindings `ctrl+s` and `ctrl+o`.

```toml
{{#include ../../../default_config.toml:45:58}}
```

## Logging

The `[logs]` section configures the application's logging behavior, which is useful for debugging or monitoring.
Note that if the `INTELLI_LOG` environment variable is set, it will override the settings in this file.

- **`enabled`**: Set to `true` to enable writing logs to a file in the application's data directory
- **`filter`**: Controls the verbosity of the logs using `tracing-subscriber` syntax

```toml
{{#include ../../../default_config.toml:64:76}}
```

Now that you've configured the application's basic behavior, you can tailor how you interact with it. Let's move on to
customizing the [**Key Bindings**](./keybindings.md).
