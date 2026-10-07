# Shell Hotkeys

Shell hotkeys are keyboard shortcuts that trigger IntelliShell features directly from your shell command line (e.g.,
Bash, Zsh, Fish, Nushell, PowerShell).

Unlike [**Key Bindings**](./keybindings.md), which control navigation and editing *inside* the Terminal User Interface
(TUI) once it is open, shell hotkeys determine how you launch IntelliShell without leaving your prompt.

## Available Hotkeys

The `[hotkeys]` section in your `config.toml` allows configuring the following actions:

| Hotkey     | Description                                                                     | Default Binding |
| ---------- | ------------------------------------------------------------------------------- | --------------- |
| `search`   | Opens the interactive search TUI to find and execute stored commands            | `ctrl-space`    |
| `bookmark` | Bookmarks the command currently in your shell prompt line buffer                | `ctrl-b`        |
| `variable` | Prompts you to fill in template variables for the command currently in the line | `ctrl-l`        |
| `fix`      | Invokes AI diagnosis and error correction on the command in the line buffer     | `ctrl-x`        |

### Default Configuration

You can customize these hotkeys in the `[hotkeys]` block of your `config.toml`:

```toml
{{#include ../../../default_config.toml:113:126}}
```

### Syntax and Validation Rules

To ensure hotkeys can be translated reliably across all supported shell line editors (Readline, ZLE, Fish, Reedline,
PSReadLine):

- **Single combination**: Hotkeys must be specified as a single string (not a list/array).
- **Supported modifiers**: Hotkeys must use either `ctrl` or `alt` combined with a single ASCII character or `space`
  (e.g., `"ctrl-space"`, `"alt-s"`, `"ctrl-p"`). Combinations like `ctrl-alt-...` or standalone keys without modifiers
  are not allowed.

## Applying Changes

Shell integration scripts generate their native key bindings dynamically when `intelli-shell init <shell>` is evaluated
during shell startup.

If you update hotkeys in your `config.toml`:

1. Save your changes to `config.toml`.
2. Reload your shell profile (e.g., `source ~/.bashrc` or `source ~/.zshrc`) or open a new terminal session.

## Environment Variable Overrides

For backwards compatibility and advanced use cases, IntelliShell also honors shell environment variables to override
hotkeys:

- `INTELLI_SEARCH_HOTKEY`: Overrides the default `ctrl-space` hotkey for searching commands
- `INTELLI_BOOKMARK_HOTKEY`: Overrides the default `ctrl-b` hotkey to bookmark a command
- `INTELLI_VARIABLE_HOTKEY`: Overrides the default `ctrl-l` hotkey for replacing variables
- `INTELLI_FIX_HOTKEY`: Overrides the default `ctrl-x` hotkey for fixing commands
- `INTELLI_SKIP_ESC_BIND=1`: Prevents IntelliShell from binding the <kbd>Esc</kbd> key to clear the current command line

If set in your shell profile before the `intelli-shell init` command, environment variables take precedence
over `config.toml`. This is especially helpful if you need shell-specific key syntax or escape sequences (such as raw
terminal escape codes or terminal emulator chords) not directly expressible in `config.toml`.

> 💡 **Tip**: For keybinding syntax when using environment variables, refer to your shell's documentation (`bindkey`
> for Zsh, `bind` for Bash). For example, to change the search hotkey in Bash using an environment variable, add
> `export INTELLI_SEARCH_HOTKEY='\C-t'` to your `.bashrc`.
>
> **Note for Fish users**: When overriding via environment variables, Fish 3.x uses escaped sequences (e.g., `\cb`),
> whereas Fish 4.x and newer require named keys (e.g., `ctrl-b` or `ctrl-l`).

---

Now that you have configured your shell entry points, you can customize the controls used inside the interface. Let's
move on to [**Key Bindings**](./keybindings.md).
