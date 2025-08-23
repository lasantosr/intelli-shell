# `new`

The `new` command bookmarks a new command in your IntelliShell library.

While commands are typically bookmarked interactively using the <kbd>Ctrl</kbd>+<kbd>B</kbd> hotkey, this command provides
a **non-interactive** way to add new entries. This is particularly useful for scripting, batch-importing from other tools,
or when you want to add a command without breaking your terminal flow.

## Usage

```sh
intelli-shell new [OPTIONS] [COMMAND_STRING]
```

## Arguments

- **`COMMAND_STRING`**
  The command to be stored. This argument is **mandatory** unless you are running in interactive mode (`-i`).
  > ⚠️ **Note**: Be mindful of shell expansion. It is best practice to wrap the command string in single quotes (`'`)
  > to ensure special characters like `$` or `&` are preserved exactly as intended.

## Options

- `-a, --alias <ALIAS>`
  
  Sets an alias (a short, memorable name) for the command.

- `-d, --description <DESCRIPTION>`
  
  Provides a detailed description for the command. You can include hashtags (`#tag`) here for organization.

- `--ai`
  
  Uses AI to generate a command and its description from the `COMMAND_STRING` prompt. This is most effective when paired
  with `-i` to review the AI's suggestions before saving.

- `-i, --interactive`
  
  Opens the interactive TUI to bookmark the command, pre-filling any details provided via other arguments and options.

- `-l, --inline`
  
  If in interactive mode, forces the TUI to render inline below the prompt.

- `-f, --full-screen`
  
  If in interactive mode, forces the TUI to render in full-screen mode.

## Examples

### Bookmark a Simple Command

Quickly save a command without any extra details. The command string is wrapped in single quotes to prevent the shell
from interpreting special characters.

```sh
intelli-shell new 'echo "Hello, IntelliShell!"'
```

### Add a Command with Full Details

For a more useful bookmark, provide an alias (`-a`) for quick searching and a description (`-d`) with hashtags for
organization.

```sh
intelli-shell new 'docker run -it --rm {{image}}' --alias 'dr' --description 'Run a temporary docker image #docker'
```

### Pre-fill the Interactive UI

Use this when you have a command ready but want to use the TUI to add the alias and description. It's a middle ground
between fully manual and fully interactive bookmarking.

```sh
intelli-shell new -i 'git checkout {{branch}}'
```

### Generate a Command with AI

This is the most powerful workflow for creating new commands. Provide a description of what you want to do, and the AI
will generate the command and its details for you. The `-i` flag is highly recommended here to review and edit the
suggestions before saving.

```sh
intelli-shell new -i --ai 'undo last n commits'
```
