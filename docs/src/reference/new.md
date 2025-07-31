# `new`

The `new` command bookmarks a new command in your IntelliShell library.

While commands are typically bookmarked interactively using the `ctrl+b` hotkey, this command provides a
non-interactive way to add new entries. This is particularly useful for scripting, batch-importing from other tools, or
when you want to add a command without breaking your terminal flow.

## Usage

```sh
intelli-shell new [OPTIONS] [COMMAND_STRING]
```

## Arguments

- **`COMMAND_STRING`**
  The command to be stored. This argument is mandatory unless you are running in interactive mode (`-i`).
  > ⚠️ **Note**: Be mindful of shell expansion. It is best practice to wrap the command string in single quotes (`'`)
  > to ensure special characters like `$` or `&` are preserved exactly as intended.

## Options

- `-a, --alias <ALIAS>`
  
  Sets an alias (a short, memorable name) for the command.

- `-d, --description <DESCRIPTION>`
  
  Provides a detailed description for the command. You can include hashtags (`#tag`) here for organization.

- `-i, --interactive`
  
  Opens the interactive TUI to bookmark the command.

- `-l, --inline`
  
  If in interactive mode, forces the TUI to render inline below the prompt.

- `-f, --full-screen`
  
  If in interactive mode, forces the TUI to render in full-screen mode.

## Examples

### 1. Bookmark a Simple Command

To save a basic command without any extra details:

```sh
intelli-shell new 'echo "Hello, IntelliShell!"'
```

### 2. Add a Command with Full Details

To save a command template with an alias and a descriptive tag:

```sh
intelli-shell new 'docker run -it --rm {{image}}' --alias 'dr' --description 'Run a temporary docker image #docker'
```

### 3. Launch the Interactive UI with a Pre-filled Command

To open the interactive bookmarking window with the command already filled in, allowing you to add the alias and
description in the TUI:

```sh
intelli-shell new 'git checkout {{branch}}' --interactive
```
