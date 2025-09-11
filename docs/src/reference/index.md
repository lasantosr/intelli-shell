# Command Line Tool Reference

While IntelliShell is designed for seamless interactive use through shell hotkeys, it also features a comprehensive
command-line interface (CLI) for scripting, automation, and direct data management. This section provides a
detailed reference for every command and its available options.

The basic structure for any command is:

```sh
intelli-shell [SUBCOMMAND] [OPTIONS] [ARGS]
```

## Commands

The commands can be thought of in three main categories: **core workflow**, **data management**, and **library expansion**.
Here is a summary of all available commands, with each one detailed on its own page.

| Command                                   | Description                                                                  |
| ----------------------------------------- | ---------------------------------------------------------------------------- |
| [`new`](./new.md)                         | Bookmarks a new command, optionally using AI to generate it                  |
| [`search`](./search.md)                   | Searches stored commands or uses AI to generate new ones                     |
| [`replace`](./replace.md)                 | Replaces the variables in a command template                                 |
| [`fix`](./fix.md)                         | Executes a command and uses AI to diagnose and fix it upon failure           |
| [`import`](./import.md)                   | Imports commands from various sources, using AI for unstructured text        |
| [`export`](./export.md)                   | Exports stored user commands to a file, URL, Gist, or stdout                 |
| [`tldr`](./tldr.md)                       | Manages integration with tldr pages, allowing you to fetch or clear examples |
| [`completion`](./completion.md)           | Manages dynamic variable completions                                         |
| [`update`](./update.md)                   | Updates intelli-shell to the latest version                                  |

## Global Flags for Interactive Mode

Several commands can be run either non-interactively or through an interactive TUI by using the `-i` or `--interactive`
flag. When in interactive mode, you can also force a specific rendering style:

- `-l`, `--inline`: Forces the TUI to render inline, below the prompt
- `-f`, `--full-screen`: Forces the TUI to render in full-screen mode
