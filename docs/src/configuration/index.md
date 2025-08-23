# Configuration

IntelliShell is designed to be highly adaptable to your personal workflow and aesthetic preferences. Most of the
customization is handled through a single `config.toml` file, while shell-specific hotkeys are configured using
environment variables.

This section will guide you through all the available options to help you make IntelliShell truly your own.

## The Configuration File

All settings related to the application's behavior, appearance, and search algorithms are stored in a file named
`config.toml`.

### Configuration File Location

IntelliShell first checks if the `INTELLI_CONFIG` environment variable is set. If it is, the application will load the
configuration from that specific file path. This is useful for testing different configurations or managing portable
setups.

If the environment variable is not set, IntelliShell falls back to searching in these default locations:

- **Linux/macOS**: `~/.config/intelli-shell/config.toml`
- **Windows**: `%APPDATA%\IntelliShell\Intelli-Shell\config\config.toml`

If no configuration file is found, IntelliShell will use its built-in default settings. To get started with
customization, you can copy a section from the [default configuration file](https://github.com/lasantosr/intelli-shell/blob/main/default_config.toml)
and modify it to your liking. Any setting you don't explicitly define will automatically use its default value.

### Configuration Topics

This section is broken down into the following chapters:

- **[General](./general.md)**: A detailed look at the `config.toml` file structure and its general settings,
  including data directory, Gist integration, and logging

- **[Key Bindings](./keybindings.md)**: Learn how to customize the keyboard shortcuts used to navigate and interact with
  the TUI

- **[Theming](./theming.md)**: Change the colors, styles, and symbols of the interface to match your terminal's theme

- **[Search Tuning](./search_tuning.md)**: An advanced guide to modifying the ranking algorithms that determine how
  commands and variable suggestions are sorted

- **[AI Integration](./ai.md)**: Learn how to connect IntelliShell to AI providers like OpenAI or local Ollama models.
  This chapter covers setting up API keys, choosing models, and customizing prompts to power features like command
  generation and error fixing.

## Shell Hotkey Configuration

The primary hotkeys that trigger IntelliShell from your command line (e.g., <kbd>Ctrl</kbd>+<kbd>Space</kbd>) are
configured separately via environment variables in your shell's profile (e.g., `~/.bashrc`, `~/.zshrc`). This is
covered in detail in the [**Installation**](../guide/installation.md#customizing-shell-integration) chapter.

---

Ready to start customizing? Let's dive into [**General**](./general.md).
