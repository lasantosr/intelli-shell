# IntelliShell

Like IntelliSense, but for shells!

![intelli-shell demo](assets/intellishell.gif)

IntelliShell acts like a bookmark store for commands, so you don't have to keep your history clean in order to be able
to find something useful with `ctrl + R`.

It currently works on Bash and Zsh and should be compatible with most Linux, Windows and MacOS.

## TL;DR

1. Install the binaries:

   ```sh
   curl -sSf https://raw.githubusercontent.com/lasantosr/intelli-shell/main/install.sh | $SHELL
   ```

2. Bookmark your first command by typing it on a terminal and using `ctrl + b`

3. _(optional)_ Run `intelli-shell fetch` to download commands from [tldr](https://github.com/tldr-pages/tldr)

4. Hit `ctrl + space` to begin the journey!

## Features

- Standalone binaries
- Search UI to autocomplete currently typed command
  - Full Text Search in both command and description
- Inline and full-screen interfaces
- Fetch command to parse and store [tldr](https://github.com/tldr-pages/tldr) pages (Thanks to them!)
- Portability. You can use bookmarked commands in any supported shell, as well as exporting and importing elsewhere.

## Installation

Remember to bookmark some commands or fetch them after the installation!

### Prebuilt

To install using prebuilt binaries:

```sh
curl -sSf https://raw.githubusercontent.com/lasantosr/intelli-shell/main/install.sh | $SHELL
```

### From source code

If your platform is not supported, you can also install using _cargo_, which is recommended to be installed using [rustup](https://www.rust-lang.org/tools/install).

```sh
cargo install intelli-shell --locked
```

You'll need to download the source script also:

```sh
mkdir -p ~/.local/share/intelli-shell
curl -sSf https://raw.githubusercontent.com/lasantosr/intelli-shell/main/intelli-shell.sh > ~/.local/share/intelli-shell/intelli-shell.sh
```

After that, you should edit your `~/.bashrc`, `~/.zshrc` or `~/.bash_profile` to source it:

```sh
source ~/.local/share/intelli-shell/intelli-shell.sh
```

## Usage

You can view supported actions by running `intelli-shell -h`. Most used standalone commands are:

- `intelli-shell fetch [category]` to fetch [tldr](https://github.com/tldr-pages/tldr) commands and store them.
   _[category]_ can be skipped or a valid folder from tldr's [pages](https://github.com/tldr-pages/tldr/tree/main/pages)
- `intelli-shell export` to export user-bookmarked commands (won't export _tldr's_ commands)
- `intelli-shell import user_commands.txt` to import commands into the user category

### Hotkeys

- `Ctrl + space` to open search UI with the current line pre-populated as the filter
  - When navigating commands, the currently selected command can be deleted with `Ctrl + d`
- `Ctrl + b` to bookmark the currently typed command
- `esc` to clean current line, this binding can be skipped if `INTELLI_SKIP_ESC_BIND=1`

You can customize key bindings using environment variables: `INTELLI_SEARCH_HOTKEY` and `INTELLI_SAVE_HOTKEY`

## Wishlist

- [ ] Labels support to store most used labels and select them using a dedicated UI
- [ ] Usability improvements to manage stored commands
- [ ] Sync user bookmarks using some public / private Git repo
- [ ] Support for more terminals, like PowerShell

## Alternatives

You might want to have a look at [Marker](https://github.com/pindexis/marker) which is pretty similar but requires Python
to be installed on your system.

## License

IntelliShell is licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for the full license text.
