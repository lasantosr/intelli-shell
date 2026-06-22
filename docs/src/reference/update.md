# `update`

The `update` command checks for a new version of IntelliShell and, if one is available, attempts to automatically update
the application.

This command provides a convenient way to stay on the latest version without needing to re-run the installation script manually.

## Usage

```sh
intelli-shell update [OPTIONS]
```

### Options

- `--to <VERSION>`: Specific version to update/downgrade to (e.g., `1.2.3` or `v1.2.3`). If not specified, updates to the latest version.

## How It Works

The update process depends on how you initially installed IntelliShell:

- **Installed via Official Script**: If you installed IntelliShell using the recommended `install.sh` or `install.ps1`
  script, the `update` command will download the target release and replace your current executable with the new version.

- **Other Installation Methods**: If you installed IntelliShell using `cargo install`, by building from source,
  or through other manual methods, the automatic update will not be performed. Instead, the command will check for the
  target version and provide you with instructions on how to update it using the same method you used for the initial installation.

## Examples

### Check for and apply updates

Simply run the command without any arguments to update to the latest version. IntelliShell will handle the rest.

```sh
intelli-shell update
```

### Update or downgrade to a specific version

To update, downgrade, or reinstall a specific version of the application:

```sh
intelli-shell update --to 0.2.0
```
