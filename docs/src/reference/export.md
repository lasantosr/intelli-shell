# export

The `export` command allows you to share or back up your user-defined commands by writing them to an external location.
This is useful for moving your library between machines, sharing it with teammates, or integrating it with version control.

By default, commands are exported in a simple, human-readable format, but the primary use case is to create backups or
share your library.

> 📝 **Note**: This command only exports your personal, bookmarked commands. Examples fetched from `tldr` pages or
> workspace commands from `.intellishell` file are not included.

## Usage

```sh
intelli-shell export [OPTIONS] [LOCATION]
```

## Arguments

- `[LOCATION]`

  Specifies the destination for the exported commands. This can be a file path, an HTTP(S) URL, or a GitHub Gist ID/URL.
  If omitted or set to `-`, the output is written to standard output (stdout), which is useful for piping to other tools.

  - **File**: `intelli-shell export ./my_commands.json`
  - **HTTP**: `intelli-shell export https://my-server.com/api/commands`
  - **Gist**: `intelli-shell export https://gist.github.com/user/1a2b3c4d5e6f7g8h9i0j`

## Options

- `--file`, `--http`, `--gist`

  Forces IntelliShell to treat the `LOCATION` as a specific type. This is useful if the location string is ambiguous.
  For example, `intelli-shell export --gist 12345` will export to a gist with id `12345` instead of treating it as a file.

- `--filter <REGEX>`

  Exports only the commands whose command string or description matches the provided regular expression. This is a
  powerful way to export specific subsets of your library.

  ```sh
  # Export only commands with the #docker or #k8s tag
  intelli-shell export --filter "#(docker|k8s)" > docker_commands.json
  ```

- `-X, --request <METHOD>`

  Specifies the HTTP method to use when the `LOCATION` is an HTTP(S) URL.
  - **Default**: `PUT`
  - **Allowed values**: `GET`, `POST`, `PUT`, `PATCH`

- `-H, --header <KEY: VALUE>`

  Adds a custom HTTP header to the request when exporting to an HTTP(S) URL. This can be specified multiple times.

  ```sh
  intelli-shell export --http https://my-api/commands -H "Authorization: Bearer my-token"
  ```

## Examples

- **Export all commands to a local file**:

  ```sh
  intelli-shell export my_commands.json
  ```

- **Export to a private GitHub Gist**:

  Requires a GitHub personal access token with the `gist` scope. The token can be provided via the `GIST_TOKEN`
  environment variable or directly in the configuration file.

  ```sh
  # GIST_TOKEN is set in the environment
  intelli-shell export --gist 1a2b3c4d5e6f7g8h9i0j
  ```

- **Export commands tagged with `#gcp` to standard output**:

  ```sh
  intelli-shell export --filter "#gcp"
  ```

- **Send commands to a custom server using `POST`**:

  ```sh
  intelli-shell export --http https://my-backup-service.com/store -X POST
  ```
