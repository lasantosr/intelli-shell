# `tldr fetch`

The `tldr fetch` command downloads command examples from the official [tldr pages](https://github.com/tldr-pages/tldr)
repository and imports them into your IntelliShell library.

This is a great way to quickly populate your library with a vast collection of common and useful commands. The imported
examples are stored in a separate `tldr` category so they don't mix with your personal bookmarks. Once fetched, they will
appear in your search results, giving you instant access to a massive library of commands.

> 💡 **Tip**: You can re-run this command at any time to update your local copy of the tldr pages to the latest
> version.

## Usage

```sh
intelli-shell tldr fetch [OPTIONS] [CATEGORY]
```

## Arguments

- **`CATEGORY`** Specifies which `tldr` category (a collection of pages) to fetch.

  If omitted, IntelliShell automatically fetches the `common` pages as well as the pages for your current operating system
  (e.g., `linux`, `osx`, or `windows`).

  For a full list of available categories, you can visit the
  [tldr pages repository](https://github.com/tldr-pages/tldr/tree/main/pages).

## Options

- `-c, --command <COMMAND_NAME>`
  
  Fetches examples for one or more specific commands, regardless of platform. This option can be repeated to specify
  multiple commands.

- `-C, --filter-commands [FILE_OR_STDIN]`
  
  Fetches examples only for commands listed in a file or from standard input. If no path is provided, it reads from
  `stdin`. Command names should be separated by newlines.

## Examples

### Fetch Default Pages for Your System

Running the command without arguments is the easiest way to get started. It fetches the most relevant pages for your
environment.

```sh
intelli-shell tldr fetch
```

### Fetch a Specific Platform

If you only want pages from a specific platform, like `common`, you can specify it as an argument.

```sh
intelli-shell tldr fetch common
```

### Fetch Pages for Specific Tools

If you only need examples for a particular command, use the `--command` flag.

```sh
intelli-shell tldr fetch --command git --command docker
```
