# `config`

The `config` command opens the intelli-shell configuration file in your default editor, or displays the path to it.

## Usage

```sh
intelli-shell config [OPTIONS]
```

## Options

- `-p, --path`

  Displays the path to the configuration file instead of opening it.

## Examples

### Open the configuration file

This will launch the default editor to modify your `config.toml`.

```sh
intelli-shell config
```

### Show the path to the configuration file

This is useful for scripting or for finding where your configuration is located.

```sh
intelli-shell config --path
```
