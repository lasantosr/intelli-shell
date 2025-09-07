# `completion list`

The `list` subcommand displays all configured dynamic variable completions, allowing you to see what completions are
available and how they are configured.

## Usage

```sh
intelli-shell completion list [OPTIONS] [COMMAND]
```

## Arguments

- **`COMMAND`** If provided, the list will be filtered to show only completions for that command.

## Options

- `-i, --interactive`

  Opens an interactive list of variable completions.

- `-l, --inline`
  
  If in interactive mode, forces the TUI to render inline below the prompt.

- `-f, --full-screen`
  
  If in interactive mode, forces the TUI to render in full-screen mode.

## Examples

### List all variable completions

This will list all completions, both global and command-specific, in a non-interactive format.

```sh
intelli-shell completion list
```

### List variable completions for a specific command

This will list only the completions that are configured for the `git` command.

```sh
intelli-shell completion list git
```

### Open the interactive list of completions

This will open an interactive TUI to browse, edit, and delete completions, while previewing their suggestions.

```sh
intelli-shell completion list -i
```
