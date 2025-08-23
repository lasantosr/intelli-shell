# `import`

The `import` command is the counterpart to `export`. It allows you to add commands to your library from an external
source, such as a file, an HTTP endpoint, or a GitHub Gist. This is the primary way to restore backups or onboard
commands shared by others.

When importing, IntelliShell merges the incoming commands with your existing library. If a command with the exact same
command string already exists, it is skipped to prevent duplicates.

## Usage

```sh
intelli-shell import [OPTIONS] [LOCATION]
```

## Arguments

- **`LOCATION`**
  
  Specifies the source of the commands to import. This can be a file path, an HTTP(S) URL, or a GitHub Gist ID/URL.

  If omitted or set to `-`, IntelliShell reads from standard input (`stdin`), allowing you to pipe data into it.

---

## Options

- `--file`, `--http`, `--gist`
  
  Forces IntelliShell to treat the `LOCATION` as a specific type, which is useful if the location string is ambiguous
  (e.g., a numeric Gist ID).

- `--filter <REGEX>`
  
  Imports only the commands from the source whose content or description matches the provided regular expression.

- `-t, --add-tag <TAG>`
  
  Appends one or more hashtags to the description of every imported command. This is a convenient way to categorize a
  new set of commands and can be specified multiple times.

- `--dry-run`
  
  Performs a "trial run" of the import. Commands are parsed and displayed but are **not** saved to your library, which is
  useful for inspecting a source before committing.

- `--ai`
  
  Uses AI to parse and extract command templates from unstructured text sources like web pages or shell history.

- `--history <SHELL>`
  
  Imports shell history (`bash`, `zsh`, `fish`, `powershell` or `atuin`). This option **requires** the `--ai` flag.

- `-i, --interactive`
  
  Opens an interactive TUI to review, edit, and select commands before importing. You can use <kbd>Space</kbd> /
  <kbd>Ctrl</kbd>+<kbd>Space</kbd> to discard or include highlighted / all commands.

- `-X, --request <METHOD>`
  
  Specifies the HTTP method to use for an HTTP(S) `LOCATION` (default: `GET`).

- `-H, --header <KEY: VALUE>`
  
  Adds a custom HTTP header to the request for an HTTP(S) `LOCATION`. This can be specified multiple times.

## Examples

### Import from a Local File

Restore your library from a local backup file.

```sh
intelli-shell import my_commands.bak
```

### Import from a Public Gist

Onboard a set of shared commands from a teammate or the community.

```sh
intelli-shell import https://gist.github.com/lasantosr/137846d029efcc59468ff2c9d2098b4f
```

### Preview Commands Before Importing

Use `--dry-run` to safely inspect the contents of a remote file without modifying your library.

```sh
intelli-shell import --dry-run https://config.my-company.com/shared-commands
```

### Convert Shell History into Bookmarks with AI

This is a powerful way to convert your most-used historical commands into a permanent, searchable library. The `-i` flag
is highly recommended to curate the results.

```sh
intelli-shell import -i --ai --history bash
```

### Extract Commands from a Web Page with AI

Turn any online cheatsheet or tutorial into a source of ready-to-use command templates. The AI will parse the page and
extract commands for you to review and import.

```sh
intelli-shell import -i --ai https://www.example.com/cheatsheet
```
