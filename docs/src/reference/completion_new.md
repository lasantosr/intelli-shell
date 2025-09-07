# `completion new`

The `new` subcommand adds a new dynamic completion for a variable. This allows you to specify a shell command that will
be executed to generate a list of possible values for a variable.

>💡 **Tip**: Can't figure out the exact command details? While in the interactive TUI, you can press <kbd>Ctrl</kbd>+<kbd>I</kbd>
> or <kbd>Ctrl</kbd>+<kbd>X</kbd> with a natural language description on the provider field to prompt AI for a command.

## Usage

```sh
intelli-shell completion new [OPTIONS] [VARIABLE] [PROVIDER]
```

## Arguments

- **`VARIABLE`** The name of the variable to provide completions for.
  
  This argument is **mandatory** unless you are running in interactive mode (`-i`).

- **`PROVIDER`**
  The shell command that generates the suggestion values when executed. The command should output a list of values separated
  by newlines.
  
  This argument is **mandatory** unless you are running in interactive mode (`-i`) or have enabled ai mode (`--ai`).
  > ⚠️ **Note**: Be mindful of shell expansion. It is best practice to wrap the command string in single quotes (`'`)
  > to ensure special characters like `$` or `&` are preserved exactly as intended.

## Options

- `-c, --command <COMMAND>`
  
  The root command where this completion must be triggered. If not provided, the completion will be global and will be
  triggered for any command that uses the specified variable.

- `--ai`
  
  Uses AI to suggest the completion command. This is most effective when paired with `-i` to review the AI's suggestions
  before saving.

- `-i, --interactive`
  
  Opens the interactive TUI to add a new dynamic completion.

- `-l, --inline`
  
  If in interactive mode, forces the TUI to render inline below the prompt.

- `-f, --full-screen`
  
  If in interactive mode, forces the TUI to render in full-screen mode.

## Examples

### Add a new completion for a global variable

This will provide suggestions for any `{{remote}}` variable, regardless of the command it's used in.

```sh
intelli-shell completion new remote "git remote"
```

### Add a new completion for a command-specific variable

This will provide suggestions for the `{{branch}}` variable only when it's used in a `git` command.

```sh
intelli-shell completion new --command git branch "git branch --format='%(refname:short)'"
```

### Use AI to suggest a completion for a variable

This will open the interactive UI and use AI to suggest a command that can provide completions for the `container` variable.

```sh
intelli-shell completion new -i --ai container
```
