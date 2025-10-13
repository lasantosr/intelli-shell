# `logs`

The `logs` command displays the logs from the last execution of intelli-shell, if they were enabled.

## Usage

```sh
intelli-shell logs [OPTIONS]
```

## Options

- `-p, --path`

  Displays the path to the log file instead of the logs content.

## Examples

### Show the logs from the last execution

```sh
intelli-shell logs
```

### Show the path to the log file

```sh
intelli-shell logs --path
```
