# `replace`

The `replace` command populates variables within a command string. Its primary function is to take a command
template containing placeholders (e.g., `{{variable}}`) and fill them in, either through command-line arguments or an
interactive prompt.

This is the underlying command that powers variable substitution when you select a command from the search UI or use the
`ctrl+l` hotkey.

## Usage

```sh
intelli-shell replace [OPTIONS] [COMMAND_STRING]
```

## Arguments

- **`COMMAND_STRING`**
    The command template containing variables to be replaced. If this argument is omitted or set to `-`, the
    command will be read from standard input, allowing for piping.

## Options

- `-e, --env <KEY[=VALUE]>`
  
  Assigns a value to a variable. This option can be used multiple times for multiple variables.
  - If specified as `key=value`, it directly assigns the value.
  - If only a `key` is provided (e.g., `--env api-token`), IntelliShell will read the value from a corresponding
    environment variable (e.g., `API_TOKEN`).

- `-E, --use-env`
  
    Automatically populate any remaining variables from their corresponding environment variables. This is a
    broader version of `--env <KEY>`, giving access to all environment variables without listing them explicitly.
    Variable names are converted to `SCREAMING_SNAKE_CASE` to find a matching environment variable (e.g., `{{http-header}}`
    maps to `HTTP_HEADER`).

    This is always enabled on interactive mode, where variables will be presented as options.

- `-i, --interactive`
  
  Opens the interactive TUI to fill in the variables.

- `-l, --inline`
  
  When used with `--interactive`, forces the TUI to render inline.

- `-f, --full-screen`
  
  When used with `--interactive`, forces the TUI to render in full-screen mode.

## Examples

### 1. Basic Non-Interactive Replacement

Provide values for variables directly on the command line.

```sh
intelli-shell replace 'echo "Hello, {{name}}!"' --env name=World
# Output: echo "Hello, World!"
```

### 2. Using Standard Input (Piping)

Pipe a command template into `replace` to have its variables filled.

```sh
echo 'curl -H "Auth: {{token}}"' | intelli-shell replace --env token=xyz123
# Output: curl -H "Auth: xyz123"
```

### 3. Populating from Environment Variables

Use existing environment variables to populate command templates.

```sh
# Set an environment variable
export FILENAME="report.pdf"

# Use the --use-env flag to automatically find and replace {{filename}}
intelli-shell replace 'tar -czvf archive.tar.gz {{filename}}' --use-env
# Output: tar -czvf archive.tar.gz report.pdf
```

### 4. Launching the Interactive UI

If you prefer to fill in variables using the TUI, use the `--interactive` flag.

```sh
intelli-shell replace 'scp {{file}} {{user}}@{{host}}:/remote/path' -i
```

This will open an interactive prompt asking you to provide values for `file`, `user`, and `host`.
