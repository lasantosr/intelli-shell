# import

The `import` command is the counterpart to `export`. It allows you to add commands to your library from an external
source, such as a file, an HTTP endpoint, or a GitHub Gist. This is the primary way to restore backups or onboard
commands shared by others.

When importing, IntelliShell merges the incoming commands with your existing library. If a command with the string
already exists, it is skipped to prevent duplicates.

## Usage

```sh
intelli-shell import [OPTIONS] [LOCATION]
```

## Arguments

- `[LOCATION]`

  Specifies the source of the commands to import. This can be a file path, an HTTP(S) URL, or a GitHub Gist ID/URL.
  If omitted or set to `-`, IntelliShell reads from standard input (stdin), allowing you to pipe data into it.

  - **File**: `intelli-shell import ./shared_commands.json`
  - **HTTP**: `intelli-shell import https://example.com/commands.json`
  - **Gist**: `intelli-shell import https://gist.github.com/user/1a2b3c4d5e6f7g8h9i0j`
  - **Stdin**: `cat commands.json | intelli-shell import`

## Options

- `--file`, `--http`, `--gist`

  Forces IntelliShell to treat the `LOCATION` as a specific type. This is useful if the location string is ambiguous.

- `--filter <REGEX>`

  Imports only the commands from the source whose command string or description matches the provided regular expression.

  ```sh
  # Import only commands related to 'git' from a shared file
  intelli-shell import --filter "git" team_commands.json
  ```

- `-t, --add-tag <TAG>`

  Appends one or more hashtags to the description of every command being imported. This is a convenient way to
  categorize a new set of commands. This option can be specified multiple times.

  ```sh
  # Import commands from a file and tag them all with #networking and #vpn
  intelli-shell import --add-tag networking --add-tag vpn company_tools.json
  ```

- `--dry-run`

  Performs a "trial run" of the import process. The commands from the source are parsed and displayed in the terminal
  but are **not** saved to your library. This is useful for inspecting the commands before committing to the import.

- `-X, --request <METHOD>`

  Specifies the HTTP method to use when the `LOCATION` is an HTTP(S) URL.
  - **Default**: `GET`
  - **Allowed values**: `GET`, `POST`, `PUT`, `PATCH`

- `-H, --header <KEY: VALUE>`

  Adds a custom HTTP header to the request when importing from an HTTP(S) URL. This can be specified multiple times.

  ```sh
  # Import from a private URL that requires authentication
  intelli-shell import --http https://my-api/commands -H "Authorization: Bearer my-token"
  ```

## Examples

- **Import commands from a local file**:

  ```sh
  intelli-shell import my_commands_backup.json
  ```

- **Import commands from a public GitHub Gist URL**:

  ```sh
  intelli-shell import https://gist.github.com/user/1a2b3c4d5e6f7g8h9i0j
  ```

- **Preview commands from a URL before importing**:

  ```sh
  intelli-shell import --dry-run https://config.my-company.com/shared-commands
  ```
