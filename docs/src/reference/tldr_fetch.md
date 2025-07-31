# tldr fetch

The `tldr fetch` command downloads command examples from the official [tldr pages](https://github.com/tldr-pages/tldr)
repository and imports them into your IntelliShell library.

This is a great way to quickly populate your library with a vast collection of common and useful commands. The imported
commands are stored in a separate `tldr` category, so they don't mix with your personal bookmarks unless you want them
to.

## Usage

```sh
intelli-shell tldr fetch [OPTIONS] [CATEGORY]
```

## Arguments

- **`[CATEGORY]`**: Specifies which `tldr` category to fetch. If omitted, IntelliShell will automatically fetch the
  `common` pages as well as the pages for your current operating system (e.g., `linux`, `osx`, or `windows`).

  For a full list of available categories, you can visit the
  [tldr pages repository](https://github.com/tldr-pages/tldr/tree/main/pages).

## Options

- **`-c, --command <COMMAND_NAME>`**: Fetches examples for one or more specific commands. This option can be repeated to
  specify multiple commands.
  
  ```sh
  # Fetch examples for git and docker
  intelli-shell tldr fetch --command git --command docker
  ```

- **`-C, --filter-commands [FILE_OR_STDIN]`**: Fetches examples for commands listed in a file or from standard input.
  If no path is provided, it reads from `stdin`. Command names should be separated by newlines.

  ```sh
  # Fetch commands listed in a file named 'my_tools.txt'
  intelli-shell tldr fetch --filter-commands my_tools.txt

  # Pipe a list of commands to fetch
  echo -e "tar\nzip" | intelli-shell tldr fetch --filter-commands
  ```

## Examples

- **Fetch default pages for your system**:

  ```sh
  intelli-shell tldr fetch
  ```

- **Fetch only the common pages**:

  ```sh
  intelli-shell tldr fetch common
  ```

- **Fetch pages for a specific tool**:

  ```sh
  intelli-shell tldr fetch --command ffmpeg
  ```
