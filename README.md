# IntelliShell

Like IntelliSense, but for shells!

![intelli-shell demo](assets/intellishell.gif)

IntelliShell acts like a bookmark store for commands, so you don't have to keep your history clean in order to be able
to find something useful with `ctrl + R`.

It currently works on Bash, Zsh and Fish and should be compatible with most Linux, Windows and MacOS.

## TL;DR

1. Install the binaries:

   ```sh
   curl -sSf https://raw.githubusercontent.com/lasantosr/intelli-shell/main/install.sh | bash
   ```

2. Bookmark your first command by typing it on a terminal and using `ctrl + b`

3. _(optional)_ Run `intelli-shell fetch` to download commands from [tldr](https://github.com/tldr-pages/tldr)

4. Hit `ctrl + space` to begin the journey!

## Features

- Standalone binaries
- Autocomplete currently typed command
  - Full Text Search in both command and description with hashtag support
- Find & replace labels of currently typed command
- Non-intrusive (inline) and full-screen interfaces
- Fetch command to parse and store [tldr](https://github.com/tldr-pages/tldr) pages (Thanks to them!)
- Portability. You can use bookmarked commands in any supported shell, as well as exporting and importing elsewhere.

## Installation

Remember to bookmark some commands or fetch them after the installation!

### Bash (Linux)

```sh
curl -sSf https://raw.githubusercontent.com/lasantosr/intelli-shell/main/install.sh | bash
```

After installing it using bash, it should work in any supported shell.

### PowerShell (Windows)

```powershell
New-Item -Path $env:APPDATA\IntelliShell\Intelli-Shell\bin -Type Directory
Invoke-WebRequest -UseBasicParsing -URI "https://github.com/lasantosr/intelli-shell/releases/latest/download/intelli-shell-x86_64-pc-windows-msvc.zip" -OutFile .\intelli-shell.zip
Expand-Archive -Path intelli-shell.zip -DestinationPath $env:APPDATA\IntelliShell\Intelli-Shell\bin
Remove-Item intelli-shell.zip
$Path = [Environment]::GetEnvironmentVariable("PATH", "User") 
if ($Path -NotLike "*IntelliShell*") { 
  $Path = $Path + [IO.Path]::PathSeparator + "$env:APPDATA\IntelliShell\Intelli-Shell\bin"
  [Environment]::SetEnvironmentVariable( "Path", $Path, "User" )
}
```

After installing it using PowerShell, it should also work in cmd.

### Source

To install from source you'll need to have Rust installed, which is recommended to be installed using [rustup](https://www.rust-lang.org/tools/install).

```sh
cargo install intelli-shell --locked
```

<details>
  <summary>Linux considerations</summary>
  
  To include hotkey integrations with current line, you'll need to download the source script also:

  ```sh
  mkdir -p ~/.local/share/intelli-shell
  curl -sSf https://raw.githubusercontent.com/lasantosr/intelli-shell/main/intelli-shell.sh > ~/.local/share/intelli-shell/intelli-shell.sh
  ```

  Or, if using fish:

  ```sh
  mkdir -p ~/.local/share/intelli-shell
  curl -sSf https://raw.githubusercontent.com/lasantosr/intelli-shell/main/intelli-shell.fish > ~/.local/share/intelli-shell/intelli-shell.fish
  ```

  After that, you should edit your `~/.bashrc`, `~/.zshrc` or `~/.bash_profile` to source it:

  ```sh
  source ~/.local/share/intelli-shell/intelli-shell.sh
  ```

  Or, if using fish you should edit `~/.config/fish/config.fish`:

  ```sh
  source ~/.local/share/intelli-shell/intelli-shell.fish
  ```

</details>

## Usage

You can view supported actions by running `intelli-shell -h`. Most used standalone commands are:

- `intelli-shell fetch [category]` to fetch [tldr](https://github.com/tldr-pages/tldr) commands and store them.
   _[category]_ can be skipped or a valid folder from tldr's [pages](https://github.com/tldr-pages/tldr/tree/main/pages)
- `intelli-shell export` to export user-bookmarked commands (won't export _tldr's_ commands)
- `intelli-shell import user_commands.txt` to import commands into the user category

Windows users, as hotkeys are not enabled, will also need those:

- `intelli-shell search` to search for stored commands
- `intelli-shell save "my command"` to save a new command

### Hotkeys

Hotkeys are only available on Linux:

- `ctrl + b` bookmark currently typed command
- `ctrl + space` show suggestions for current line
- `ctrl + l` replace labels of currently typed command
- `esc` clean current line, this binding can be skipped if `INTELLI_SKIP_ESC_BIND=1`

**Note:** When navigating items, selected suggestion can be deleted with `ctrl + d`

You can customize key bindings using environment variables: `INTELLI_SAVE_HOTKEY`, `INTELLI_SEARCH_HOTKEY` and `INTELLI_LABEL_HOTKEY`

## Wishlist

- [x] Labels support to store most used labels and select them using a dedicated UI
- [ ] Usability improvements to manage stored commands (including aliases)
- [x] Support for more terminals
  - [x] [Fish](https://fishshell.com/)
  - [x] PowerShell
- [ ] Export also labels and UI to filter what to export
- [ ] Deploy to package managers
- [ ] Sync user bookmarks using some public / private Git repo

## Alternatives

You might want to have a look at [Marker](https://github.com/pindexis/marker) which is pretty similar but requires Python
to be installed on your system.

## License

IntelliShell is licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for the full license text.
