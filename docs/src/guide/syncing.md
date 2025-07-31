# Syncing and Sharing

IntelliShell makes it easy to back up your command library, share it with teammates, or sync it across multiple
machines. This is all handled by the `intelli-shell export` and `intelli-shell import` commands.

You can export your commands to a local file, a remote HTTP endpoint, or a GitHub Gist, and then import them elsewhere.
The tool automatically detects the location type (file, http, or gist) based on the provided string, but you can also
specify it explicitly with the `--file`, `--http`, or `--gist` flags.

Commands are stored in a simple, human-readable text format. Any commented lines (`#`) directly preceding a command are
treated as its description, making the files easy to edit by hand.

```sh
# A multi-line description for a command
# with a #hashtag for organization.
docker ps --format "table {{.ID}}\t{{.Image}}\t{{.Status}}"

# Prints the current working directory
pwd
```

---

## Local Backup & Restore

The simplest way to back up your commands is by exporting them to a local file. This creates a portable snapshot of
your library that you can store or move to another machine.

### 1. Backing Up to a File

To export all your user-defined commands into a single file, provide the file path to the `export` command.

```sh
intelli-shell export my_commands.bak
```

### 2. Restoring from a File

To restore your commands from a backup file, provide the file path to the `import` command.

```sh
intelli-shell import my_commands.bak
```

---

## Syncing with a GitHub Gist

Using a GitHub Gist is a flexible way to manage your command library. You can use a **private** Gist for personal cloud
sync across your devices, or a **public** Gist to share useful commands with the community. It's also an effective
method for sharing project-related commands with teammates.

### Supported Gist Locations

IntelliShell can understand various Gist location formats:

* **Full URL**: A standard URL to a Gist (e.g., `https://gist.github.com/user/gist-id`)
* **Shorthand ID**: Just the unique ID of the Gist (e.g., `b3a462e23db5c99d1f3f4abf0dae5bd8`)
* **ID and File**: You can target a specific file within a Gist by appending the filename (e.g., `gist-id/commands.sh`)

### 1. Exporting to a Gist

Before you can export to a Gist, you must first create it on GitHub to get its unique ID. IntelliShell updates existing
Gists; it does not create new ones.

When specifying the Gist location:

* **Using a URL**: IntelliShell automatically detects that the location is a Gist
* **Using an ID**: If you use just the Gist ID, you **must** add the `--gist` flag to distinguish it from a local file
  with the same name

```sh
# The --gist flag is required when using only the ID
intelli-shell export --gist {{gist-id}}
```

> **Gist Authentication**: To export to a Gist, you need a GitHub Personal Access Token with `gist` scope. You can set
> this using the `GIST_TOKEN` environment variable or in your `config.toml` file. For more details, see the
> [**Configuration**](../configuration/index.md) chapter.

### 2. Importing from a Gist

Similarly, you can import from the Gist on another machine to sync your commands.

```sh
# The --gist flag is also required here when using only the ID
intelli-shell import --gist {{gist-id}}
```

> **Tip: Set a Default Gist**
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

---

## Syncing with a Custom HTTP Endpoint

If you prefer to host your own command storage, you can configure IntelliShell to sync with any custom HTTP endpoint you
control. This is ideal for teams who want to maintain a private, centralized command library on their own infrastructure.

When exporting, IntelliShell sends a `PUT` request with a JSON payload of your commands. When importing, it can handle
either the standard plain text format (`Content-Type: text/plain`) or a JSON array (`Content-Type: application/json`).
You can also specify custom headers for authentication.

```sh
# Export to a private, authenticated endpoint
intelli-shell export -H "Authorization: Bearer {{{private-token}}}" https://my-server.com/commands

# Import from the same endpoint
intelli-shell import -H "Authorization: Bearer {{{private-token}}}" https://my-server.com/commands
```

---

## Advanced Options

Here are a few more options to customize your import and export workflows.

### Filtering Commands

The `--filter` flag lets you process a subset of commands using a regular expression. This works for both importing and
exporting.

```sh
# Export only docker commands to a local file
intelli-shell export docker_commands.sh --filter "^docker"

# Import only git commands from a file
intelli-shell import all_commands.sh --filter "^git"
```

### Tagging on Import

When importing commands from a shared source, you can use `--add-tag` (`-t`) to automatically organize them.

```sh
# Import commands for a specific project, tagging them with #project
intelli-shell import -t project https://gist.githubusercontent.com/user/id/raw/project.sh
```

### Previewing with Dry Run

If you're not sure what a file or URL contains, use the `--dry-run` flag with the `import` command. It will print the
commands that would be imported to the terminal without actually saving them to your library.

```sh
intelli-shell import --dry-run https://example.com/some-commands.sh 
```

Now that you're familiar with syncing, let's look at how to customize IntelliShell in the
[**Configuration**](../configuration/index.md) section.
