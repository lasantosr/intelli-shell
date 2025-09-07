# `export`

The `export` command allows you to share or back up your user-defined commands and completions by writing them to an
external location. This is useful for moving your library between machines or sharing it with teammates.

By default, commands and completions are exported in a simple, human-readable text format.

> 📝 **Note**: This command only exports your personal, bookmarked commands and completions. Examples fetched from `tldr`
> pages or workspace-specific items from `.intellishell` file are not included.

## Usage

```sh
intelli-shell export [OPTIONS] [LOCATION]
```

## Arguments

- **`LOCATION`** Specifies the destination for the exported commands.
  
  This can be a file path, an HTTP(S) URL, or a GitHub Gist ID/URL. If omitted or set to `-`, the output is written to
  standard output (`stdout`), which is useful for piping.

## Options

- `--file`, `--http`, `--gist`
  
  Forces IntelliShell to treat the `LOCATION` as a specific type. This is useful if the location string is ambiguous
  (e.g., `12345`), to distinguish between a local file and a Gist ID.

- `--filter <REGEX>`
  
  Exports only the commands whose content or description matches the provided regular expression.

  When commands are filtered, only completions for variables present on those commands are exported.

- `-X, --request <METHOD>`
  
  Specifies the HTTP method to use when the `LOCATION` is an HTTP(S) URL (default: `PUT`).

- `-H, --header <KEY: VALUE>`
  
  Adds a custom HTTP header to the request when exporting to an HTTP(S) URL. This can be specified multiple times.

- `-i, --interactive`
  
  Opens an interactive TUI to review, edit, and select specific commands before exporting. In this interface, you can
  update commands before exporting and use <kbd>Space</kbd> / <kbd>Ctrl</kbd>+<kbd>Space</kbd> to discard or include
  highlighted / all commands.

## Examples

### Export All Commands to a File

This is the simplest way to create a local backup of your library.

```sh
intelli-shell export my_commands.bak
```

### Export to a Private GitHub Gist

To sync your library across machines, you can export to a Gist. This requires a GitHub Personal Access Token with the
`gist` scope, provided via the `GIST_TOKEN` environment variable or in your config file.

```sh
# GIST_TOKEN is set in the environment
intelli-shell export --gist 1a2b3c4d5e6f7g8h9i0j
```

### Export a Subset of Commands

Use `--filter` to export only the commands you need. This example exports commands tagged with `#gcp` (and any completion
applicable to those commands) to standard output.

```sh
intelli-shell export --filter "#gcp"
```

### Send to a Custom Server

You can integrate IntelliShell with your own infrastructure by exporting to a custom HTTP endpoint.

```sh
intelli-shell export -H "Authorization: Bearer my-token" -X POST https://my-api/commands 
```

### Interactively Select Commands to Export

For fine-grained control, use interactive mode. This example finds all commands related to "docker" and then opens a
TUI where you can hand-pick which ones to save to the file.

```sh
intelli-shell export -i --filter "docker" docker_commands.bak
```
