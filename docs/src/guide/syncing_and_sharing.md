# Syncing and Sharing

IntelliShell makes it easy to back up your library, share it with teammates, or sync it across multiple machines.
This is all handled by the `intelli-shell export` and `intelli-shell import` commands.

You can export your commands and completions to a local file, a remote HTTP endpoint, or a GitHub Gist, and then import
them elsewhere. The tool automatically detects the location type (file, http, or gist) based on the provided string, but
you can also specify it explicitly.

## File Format

IntelliShell uses a simple, human-readable text format that makes it easy to view and edit your commands manually. The
parser is designed to be flexible, allowing it to understand not only its official format but also variations found in
sources like `tldr` pages or other shell script files.

A command is any line that isn't a blank line or a comment. To add a **description**, simply place one or more comment
lines directly above the command (comment lines can start with `#` or `//`). For better organization, you
can also include an **alias** within the description by adding `[alias:your-alias]` at the beginning or end of the
comment block.

If you have a long command, you can split it across **multiple lines** by ending each line (except the last) with a
backslash (`\`).

**Dynamic completions** are also supported. Any line starting with a dollar sign (`$`) is treated as a completion
definition. Use the format `$ (command) variable: provider` for command-specific completions, or `$ variable: provider`
for global ones.

```sh
# --------------------------------------------------------------
#   Commands
# --------------------------------------------------------------

# A multi-line description for a command
# with a #hashtag for organization.
git log --oneline --graph --decorate

# [alias:tfp] Plan infrastructure changes for a specific environment.
# This command is multi-line for readability.
terraform plan \
    -var-file="envs/{{env}}.tfvars"

# --------------------------------------------------------------
#   Completions
# --------------------------------------------------------------

# A global completion for any `{{branch}}` variable.
$ branch: git branch --format='%(refname:short)'

# A command-specific completion for the `{{env}}` variable when using `terraform`.
$ (terraform) env: find envs -type f -name "*.tfvars" -printf "%P\n" | sort | sed 's/\.tfvars$//'
```

## Local Backup & Restore

The simplest way to back up your library is by exporting them to a local file. This creates a portable snapshot of
your library that you can store or move to another machine.

```sh
# Back up to a file
intelli-shell export my_commands.bak

# Restore from the file
intelli-shell import my_commands.bak
```

## Syncing with a GitHub Gist

Using a GitHub Gist is a flexible way to manage your library. You can use a **private** Gist for personal cloud sync
across your devices, or a **public** Gist to share useful commands with the community. It's also an effective method for
sharing project-related commands with teammates.

Before you can export to a Gist for the first time, you must create it on GitHub to get its unique ID.

```sh
# The --gist flag is required when the location could be mistaken for a file name
intelli-shell export --gist 137846d029efcc59468ff2c9d2098b4f/command.sh

# Or use the URL
intelli-shell import https://gist.github.com/lasantosr/137846d029efcc59468ff2c9d2098b4f
```

> **Gist Authentication**: To export to a Gist, you need a GitHub Personal Access Token with `gist` scope. You can set
> this using the `GIST_TOKEN` environment variable or in your `config.toml` file. For more details, see the
> [**Configuration**](../configuration/general.md#gist-integration) chapter.

### Supported Gist Locations

IntelliShell is flexible and can understand various Gist location formats.

#### Full URLs

You can use almost any URL related to your Gist, including the main Gist page, the raw content URL, or the API endpoint.

#### Shorthand Formats

For convenience, you can also use shorter formats (these require `--gist` flag to disambiguate):

- `{id}`: Just the unique ID of the Gist
- `{id}/{file}`: Target a specific file within the Gist
- `{id}/{sha}`: Target a specific version (commit SHA) of the Gist
- `{id}/{sha}/{file}`: Target a specific file at a specific version

> 💡 **Tip**: Set a Default Gist
>
> You can set a default Gist ID in your `config.toml` file. Once configured, you can sync with even shorter
> commands, as IntelliShell will use the default ID when it sees `"gist"` as the location:
>
> ```sh
> # Export to the default Gist
> intelli-shell export gist
>
> # Import from the default Gist
> intelli-shell import gist
> ```

## Syncing with a Custom HTTP Endpoint

If you prefer to host your own storage, you can configure IntelliShell to sync with any custom HTTP endpoint.
This is ideal for teams who want to maintain a private, centralized library on their own infrastructure.

When exporting, IntelliShell sends a `PUT` request with a JSON payload of your commands and completions. When importing,
it can handle either the standard plain text format (`Content-Type: text/plain`) or a JSON array (`Content-Type: application/json`).
You can also specify custom headers for authentication.

```sh
# Export to a private, authenticated endpoint
intelli-shell export -H "Authorization: Bearer {{{private-token}}}" https://my-server.com/commands

# Import from the same endpoint
intelli-shell import -H "Authorization: Bearer {{{private-token}}}" https://my-server.com/commands
```

## Fine-Tuning Your Workflow

Here are a few more options to customize your import and export workflows.

### Interactive Review

For more control over what gets imported or exported, you can use the `--interactive` (`-i`) flag. This opens a
terminal UI that displays a list of all commands and completions _before_ the action is performed.

In this interactive view, you can:

- **Review** every command and completion
- **Edit** a completion, command or its description on the fly
- **Discard/Undiscard** specific commands or completions by pressing <kbd>Space</kbd>

This is especially useful when importing from a new or untrusted source, or when using the AI parser, as it gives you a
final chance to clean up and validate the results.

```sh
# Interactively review the content from a file before importing
intelli-shell import -i --gist {{gist-url}}

# Interactively choose which docker commands and completions to export
intelli-shell export -i --filter docker
```

### Filtering Commands

The `--filter` flag lets you process a subset of commands using a regular expression. This works for both importing and
exporting.

```sh
# Export only docker commands to a local file
intelli-shell export --filter "^docker" docker_commands.sh
```

> ⚠️ **Note**: When exporting filtered commands, only those completions that apply to those filtered commands are exported.

### Tagging on Import

When importing commands from a shared source, you can use `--add-tag` (`-t`) to automatically organize them.

```sh
# Import commands for a specific project, tagging them with #project
intelli-shell import -t project path/to/commands.file
```

### Previewing with Dry Run

If you're not sure what a file or URL contains, use the `--dry-run` flag with the `import` command. It will print the
commands and completions that would be imported to the terminal without actually saving them to your library.

```sh
intelli-shell import --dry-run https://example.com/some-commands.sh
```
