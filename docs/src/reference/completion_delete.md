# `completion delete`

The `delete` subcommand removes an existing dynamic variable completion.

## Usage

```sh
intelli-shell completion delete [OPTIONS] <VARIABLE>
```

## Arguments

- **`<VARIABLE>`** The variable name of the completion to delete.

## Options

- `-c, --command <COMMAND>`

  The root command of the completion to delete. If not provided, it will delete the global completion for the specified variable.

## Examples

### Delete a global dynamic completion

This will delete the completion for the `container` variable that applies to all commands.

```sh
intelli-shell completion delete container
```

### Delete a command-specific dynamic completion

This will delete the completion for the `branch` variable that is specific to `git` commands.

```sh
intelli-shell completion delete --command git branch
```
