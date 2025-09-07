# Variable Completions

The `completion` subcommand is used to manage dynamic variable completions.

This feature allows you to define a shell command that, when executed, provides a list of possible values for a given
variable. This is particularly useful for variables that have a dynamic set of possible values, such as container names,
git branches, or available network interfaces.

## Available Subcommands

The `completion` functionality is split into three main subcommands:

- **[new](./completion_new.md)**: Adds a new dynamic completion for a variable
- **[list](./completion_list.md)**: Lists all configured dynamic variable completions
- **[delete](./completion_delete.md)**: Deletes an existing dynamic variable completion
