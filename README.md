# IntelliShell

_Like IntelliSense, but for shells!_

IntelliShell is a command-line tool that acts as a smart bookmark manager.
It helps you find, organize, and reuse complex shell commands without ever leaving your terminal.

Works on **Bash**, **Zsh**, **Fish**, and **PowerShell**, with standalone binaries for Linux, macOS, and Windows.

![intelli-shell demo](docs/src/images/demo.gif)

## Features

- **Standalone & Dependency-Free**: Distributed as a single binary with no external runtimes or dependencies
- **Seamless Shell Integration**: Search commands with `ctrl+space` or bookmark them with `ctrl+b`
- **Flexible Interface**: Choose between a non-intrusive (inline) or an immersive (full-screen) TUI
- **Dynamic Variables**: Create command templates with `{{variables}}` and replace them on the fly
- **Highly Configurable**: Tailor search modes, keybindings, themes, and even search-ranking algorithms
- **Workspace-Aware Commands**: Automatically discovers and loads commands from your workspace's directory
- **Import/Export**: Share your command library by importing or exporting to files, HTTP endpoints, or even Gists
- **TLDR Integration**: Fetch and import command examples from [tldr](https://github.com/tldr-pages/tldr) pages

## Quick Start

1. Install or update the binaries:

   ```sh
   # For Linux and macOS (Bash, Zsh, Fish)
   curl -sSf https://raw.githubusercontent.com/lasantosr/intelli-shell/main/install.sh | sh
   ```

   <details>
   <summary>Windows</summary>

   ```powershell
   Set-ExecutionPolicy RemoteSigned -Scope CurrentUser # Optional: Only needed if scripts are disabled
   irm https://raw.githubusercontent.com/lasantosr/intelli-shell/main/install.ps1 | iex
   ```

   > **Note**: Microsoft Visual C++ Redistributable ([download](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist))
   > is required for the application to run
   </details>

   _To skip profile updates, set `INTELLI_SKIP_PROFILE` environment variable to `1` before installing._

2. Bookmark your first command by typing it on a terminal and hitting `ctrl+b`

3. _(optional)_ Run `intelli-shell tldr fetch` to download commands from [tldr](https://github.com/tldr-pages/tldr)

4. Hit `ctrl+space` to begin the journey and dont forget to checkout the [tips](#tips)!

## Basic Usage

IntelliShell is designed to be used interactively through keybindings:

- **`ctrl+space`**: Search for a command
- **`ctrl+b`**: Bookmark the command currently typed in your terminal
- **`ctrl+l`**: Replace `{{variables}}` in the current command
- **`esc`** clean current line, this binding can be skipped if `INTELLI_SKIP_ESC_BIND=1`

You can customize everything from keybindings and themes to the search behavior.
For a complete list of available options, check out the [default configuration file](./default_config.toml).

For detailed information on installation, configuration or advanced usage examples, please refer to
the [**Book**](https://lasantosr.github.io/intelli-shell/).

## Tips

- **Quick autocomplete**: If your search query matches an alias, it will be autocompleted instantly. The variable
  replacement UI will still appear if the command has variables.

- **Alias your favorites**: Use aliases to "pin" different sets of favorite values for the same command. For example,
  bookmark `cd {{path}}` with a `cd` alias and you can regularly use `cd` but if you hit `ctrl+space` it will show your
  "pinned" folders.

- **Quickly re-prompt variables**: Need to run a command again with different inputs? Hit the up arrow in your shell to
  recall the last command, then press `ctrl+space`. You'll get the original template back, ready for new values.

- **Organize with hashtags**: Add hashtags like `#work` or `#gcp` to your command descriptions. You can then find and use
  these hashtags in your search query to quickly filter your bookmarks.

- **Bookmark everything**: Don't just bookmark simple commands! You can save entire shell functions for reuse.
  For example: `custom_echo() { echo "hey: $@"; }; custom_echo {{text}};`

- **Keep variables secret**: If you have a variable you don't want to save in your suggestion history (like a token or a
  comment), wrap its name in an extra pair of brackets: `echo "{{{message}}}"`

- **Embrace environment variables**: Let your environment do the work. Suggestions are automatically pulled from your
  environment variables. For example, `{{{api-key}}}` will suggest `$API_KEY` variable if it exists.

- **Format variables**: Apply formatting functions directly within your variable placeholders. This is perfect for
  transforming input on the fly, like a git-friendly branch name: `git checkout -b {{feature|bugfix}}/{{{description:kebab}}}`

- **Define workspace-specific commands**: Create a `.intellishell` file in your workspace's root directory and commit it
  to git. These commands are temporary, prioritized in search results, and don't clutter your global library.
  It's the perfect way to define and share common tasks (like build or deploy scripts) with your team.

- **Share your knowledge**: Found a set of commands that could help others? Use the export feature to a public Gist of
  your bookmarks. Share the link on your blog, with your team, or contribute to a curated list.

- **Name your variables wisely**: You have full control over which suggestions are shared between commands. Suggestions
  are grouped by variable name and root cmd. Use the same name to share suggestions, or different names to keep them separate.
  - **Shared**: Using `{{image}}` for both `docker run` and `docker rmi` will share the same list of image suggestions.
  - **Separate**: Keep suggestions for different use cases distinct by using different variable names. For example, you
    can separate your `ssh` connections for `{{prod_server}}` and `{{staging_server}}` environments. Giving each a distinct
    alias like `sshp` and `sshs` lets you quickly access the right set of servers.

## License

IntelliShell is licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for the full license text.
