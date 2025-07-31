# Command Line Tool Reference

While IntelliShell is designed for seamless interactive use through shell hotkeys, it also features a comprehensive
command-line interface (CLI) for scripting, automation, and direct data management. This section provides a
detailed reference for every command and its available options.

The basic structure for any command is:

```sh
intelli-shell <COMMAND> [ARGS]
```

## Commands

Here is a summary of all available commands. Each command is detailed on its own page.

| Command                                   | Description                                                                  |
| ----------------------------------------- | ---------------------------------------------------------------------------- |
| [`new`](./new.md)                         | Bookmarks a new command                                                      |
| [`search`](./search.md)                   | Searches stored commands                                                     |
| [`replace`](./replace.md)                 | Replaces the variables of a command                                          |
| [`import`](./import.md)                   | Imports user commands from a file, URL, Gist, or stdin                       |
| [`export`](./export.md)                   | Exports stored user commands to a file, URL, or Gist                         |
| [`tldr`](./tldr.md)                       | Manages integration with tldr pages, allowing you to fetch or clear examples |

## Interactive Mode Flags

Several commands (`new`, `search`, `replace`) can be run either non-interactively with arguments or through the TUI by
using the `-i` or `--interactive` flag. When in interactive mode, you can force a specific rendering style:

- `-l`, `--inline`: Forces the TUI to render inline, below the prompt
- `-f`, `--full-screen`: Forces the TUI to render in full-screen mode
