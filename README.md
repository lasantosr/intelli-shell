# IntelliShell

Like IntelliSense, but for shells!

![intelli-shell demo](assets/intellishell.gif)

IntelliShell acts like a bookmark store for commands, so you don't have to keep your history clean in order to be able
to find something useful with `ctrl + R`.

It currently works on Bash, Zsh, Fish and PowerShell and should be compatible with most Linux, Windows and MacOS.

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
  - Full Text Search in both command and description with hashtag support on descriptions
- Find & replace labels of currently typed command
- Edit bookmarked commands and provide aliases
- Non-intrusive (inline) and full-screen interfaces
- Fetch command to parse and store [tldr](https://github.com/tldr-pages/tldr) pages (Thanks to them!)
- Portability. You can use bookmarked commands in any supported shell, as well as exporting and importing elsewhere.

## Installation

Remember to bookmark some commands or fetch them after the installation!

To skip profile updates, set `INTELLI_SKIP_PROFILE` environment variable to `1` before installing.

### Bash (Linux)

```sh
curl -sSf https://raw.githubusercontent.com/lasantosr/intelli-shell/main/install.sh | bash
```

After installing it using bash, it should work in any supported shell.

### PowerShell (Windows)

```powershell
Set-ExecutionPolicy RemoteSigned -Scope CurrentUser # Optional: Needed to run a remote script the first time
irm https://raw.githubusercontent.com/lasantosr/intelli-shell/main/install.ps1 | iex
```

After installing it with powershell, it should also work on cmd (without hotkeys).

### Source

To install from source you'll need to have Rust installed, which is recommended to be installed using [rustup](https://www.rust-lang.org/tools/install).

```sh
cargo install intelli-shell --locked
```

To enable hotkeys, additional steps are required:

<details>
  <summary>Linux</summary>
  
Download source script:

- Bash / Zsh:

  ```sh
  mkdir -p ~/.local/share/intelli-shell/bin
  curl -sSf https://raw.githubusercontent.com/lasantosr/intelli-shell/main/intelli-shell.sh > ~/.local/share/intelli-shell/bin/intelli-shell.sh
  ```

- Fish:

  ```sh
  mkdir -p ~/.local/share/intelli-shell/bin
  curl -sSf https://raw.githubusercontent.com/lasantosr/intelli-shell/main/intelli-shell.fish > ~/.local/share/intelli-shell/bin/intelli-shell.fish
  ```

Edit your profile to source it:

- Bash / Zsh: `~/.bashrc`, `~/.zshrc` or `~/.bash_profile`

  ```sh
  source ~/.local/share/intelli-shell/bin/intelli-shell.sh
  ```

- Fish: `~/.config/fish/config.fish`:

  ```sh
  source ~/.local/share/intelli-shell/bin/intelli-shell.fish
  ```

</details>

<details>
  <summary>Windows</summary>
  
Download the source script also:

```powershell
New-Item -Path $env:APPDATA\IntelliShell\Intelli-Shell\data\bin -Type Directory
Invoke-WebRequest -UseBasicParsing -URI "https://raw.githubusercontent.com/lasantosr/intelli-shell/main/intelli-shell.ps1" -OutFile $env:APPDATA\IntelliShell\Intelli-Shell\data\bin\intelli-shell.ps1
```

Edit your `$Profile` to execute it:

```powershell
. $env:APPDATA\IntelliShell\Intelli-Shell\data\bin\intelli-shell.ps1
```

</details>

## Usage

You can view supported actions by running `intelli-shell -h`. Most used standalone commands are:

- `intelli-shell fetch [category]` to fetch [tldr](https://github.com/tldr-pages/tldr) commands and store them.
   _[category]_ can be skipped or a valid folder from tldr's [pages](https://github.com/tldr-pages/tldr/tree/main/pages)
- `intelli-shell export` to export user-bookmarked commands (won't export _tldr's_ commands)
- `intelli-shell import user_commands.txt` to import commands into the user category

### Hotkeys

- `ctrl + b` bookmark currently typed command
- `ctrl + space` show suggestions for current line
- `ctrl + l` replace labels of currently typed command
- `esc` clean current line, this binding can be skipped if `INTELLI_SKIP_ESC_BIND=1`

**Note:** When navigating items, selected suggestion can be deleted with `ctrl + d` or edited with any of: `ctrl + e`,
`ctrl + u` or `F2`

You can customize key bindings using environment variables: `INTELLI_BOOKMARK_HOTKEY`, `INTELLI_SEARCH_HOTKEY` and `INTELLI_LABEL_HOTKEY`

## Tips

- When the search criteria matches an alias or produces a single result, it's automatically autocompleted!
  - The label UI will still be shown if the command contains labels
- You can alias common commands to store some kind of favorite labels, for example bookmark `cd {{path}}` and give it a
  `cd` alias
  - You can regularly use `cd` but if you hit `ctrl + space` it will show your "pinned" folders
- Long commands or even functions can also be bookmarked
  - For example `function custom_echo () { echo "hey: $@"; }; custom_echo {{text}};`
- Label suggestions are stored based on the root command and the label name, which gives you flexibility to decide.

  For these two commands, the same images will be suggested:
  - `docker run {{--rm}} {{--interactive}} {{image}}`
  - `docker rmi {{--no-prune}} {{image}}`
  
  But these two commands will suggest different volumes:
  - `docker run --volume {{image-1-volumes}} image-1`
  - `docker run --volume {{image-2-volumes}} -p {{image-2-ports}} image-2`

## Wishlist

- [x] Labels support to store most used labels and select them using a dedicated UI
- [x] Usability improvements to manage stored commands (including aliases)
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
