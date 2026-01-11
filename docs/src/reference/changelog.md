# `changelog`

The `changelog` command retrieves and displays the release notes for IntelliShell versions.
This command helps you discover what's new, what's changed, and what's fixed in different versions.

## Usage

```sh
intelli-shell changelog [OPTIONS]
```

## Options

- `--from <VERSION>`: The version to start the changelog from (inclusive). Defaults to the currently installed version.
- `--to <VERSION>`: The version to end the changelog at (inclusive).
- `--major`: Show only major versions (X.0.0).
- `--minor`: Show only major and minor versions (X.Y.0).

## Examples

### View all changes since current version

Simply run the command without any arguments to see what's new since your current version.

```sh
intelli-shell changelog
```

### View changes between specific versions

You can specify a range of versions to inspect.

```sh
intelli-shell changelog --from v2.0.0 --to v2.0.1
```

### View only major releases

Filter to see only major version updates.

```sh
intelli-shell changelog --from v0.1.1 --major
```
