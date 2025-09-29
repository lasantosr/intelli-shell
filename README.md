# IntelliShell

_Like IntelliSense, but for shells!_

IntelliShell is a powerful command template and snippet manager for your shell. It goes far beyond a simple history search,
transforming your terminal into a structured, searchable, and intelligent library of your commands.

Works on **Bash**, **Zsh**, **Fish**, **Nushell**, and **PowerShell**, with standalone binaries for Linux, macOS, and
Windows.

![intelli-shell demo](docs/src/images/demo.gif)

## Features

- **Seamless Shell Integration**: Search with `ctrl+space`, bookmark with `ctrl+b` or fix with `ctrl+x`
- **Dynamic Variables**: Create command templates with `{{variables}}` and replace them on the fly
- **Smart Completions**: Power up your variables with dynamic suggestions from any command
- **AI-Powered Commands**: Generate, fix, and import commands effortlessly using local or remote LLMs
- **Highly Configurable**: Tailor search modes, keybindings, themes, and even search-ranking algorithms
- **Workspace-Aware**: Automatically discovers and loads commands from your workspace's directory
- **Import / Export**: Share your command library using files, HTTP endpoints, or even Gists
- **TLDR Integration**: Fetch and import command examples from [tldr](https://github.com/tldr-pages/tldr) pages
- **Flexible Interface**: Choose between a non-intrusive (inline) or an immersive (full-screen) TUI

## Quick Start

1. Install or update the binary:

   ```sh
   # For sh-compatible shells on Linux/macOS/Windows (Bash, Zsh, Fish, Nu, Git Bash)
   curl -sSf https://raw.githubusercontent.com/lasantosr/intelli-shell/main/install.sh | sh
   ```

   <details>
   <summary>Windows (PowerShell)</summary>

   ```powershell
   Set-ExecutionPolicy RemoteSigned -Scope CurrentUser # Optional: Only needed if scripts are disabled
   irm https://raw.githubusercontent.com/lasantosr/intelli-shell/main/install.ps1 | iex
   ```

   > **Note**: Microsoft Visual C++ Redistributable ([download](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist))
   > is required for the application to run
   </details>

   _To skip profile updates, set `INTELLI_SKIP_PROFILE` environment variable to `1` before installing._

2. Bookmark your first command by typing it on a terminal and hitting `ctrl+b`

3. _(optional)_ [Enable AI features](https://lasantosr.github.io/intelli-shell/configuration/ai.html#enabling-ai-features),
   import commands and completions from shared [gists](https://gist.github.com/search?q=intellishell+commands) or fetch
   commands from [tldr](https://lasantosr.github.io/intelli-shell/guide/basic_usage.html#from-tldr-pages)

4. Hit `ctrl+space` to begin the journey and don't forget to checkout the [tips](#tips)!

## Basic Usage

IntelliShell is designed to be used interactively through keybindings, for detailed information on installation,
configuration, or advanced usage examples, please refer to the [_**Book**_](https://lasantosr.github.io/intelli-shell/).

### Shell Integration

These hotkeys work directly in your terminal line:

- **`ctrl+space`**: Search for a command
- **`ctrl+b`**: Bookmark the command currently typed in your terminal
- **`ctrl+l`**: Replace `{{variables}}` in the current command
- **`ctrl+x`**: Diagnose and try to fix a failing command (requires AI to be enabled)
- **`esc`**: Clean the current line, this binding can be skipped by setting `INTELLI_SKIP_ESC_BIND=1`

_These keybindings can be changed, see [Customizing Shell Integration](https://lasantosr.github.io/intelli-shell/guide/installation.html#customizing-shell-integration)
for details._

### Inside the Application

Once any interface is shown, you can use these keys:

- **`esc`**: Quit without making a selection or go back
- **`F2` / `ctrl+u` / `ctrl+e`**: Edit the highlighted item
- **`ctrl+d`**: Delete the highlighted item
- **`enter` / `tab`**: Confirm a selection or move to the next step
- **`ctrl+enter` / `ctrl+r`**: Execute the highlighted command immediately
- **`ctrl+i` / `ctrl+x`**: Prompt AI (when searching or creating commands)

_These keybindings are fully customizable; in fact, you can configure everything from themes to search behavior. See
the [Keybindings Configuration](https://lasantosr.github.io/intelli-shell/configuration/keybindings.html) page for
binding specifics, or check out the [default configuration file](./default_config.toml) for a complete list of all
available options._

## Tips

- **Quick autocomplete**: If your search query matches an alias, it will be autocompleted instantly. The variable
  replacement UI will still appear if the command has variables.

- **Learn Commands on the Fly**: Can't find the command you're looking for? Just describe it in natural language and
  press `ctrl+i` while searching to let the AI write it for you.

- **Alias your favorites**: Use aliases to "pin" different sets of favorite values for the same command. For example,
  bookmark `cd {{path}}` with a `cd` alias and you can regularly use `cd` but if you hit `ctrl+space` it will show your
  "pinned" folders.

- **Quickly re-prompt variables**: Need to run a command again with different inputs? Hit the up arrow in your shell to
  recall the last command, then press `ctrl+space`. You'll get the original template back, ready for new values.

- **Fix errors instantly**: Typed a long command only to have it fail? Instead of manually debugging, just hit the up
  arrow to recall it and press `ctrl+x`. The AI will analyze the command and the error message to suggest a working version.

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

- **Define workspace-specific commands**: Create a [`.intellishell`](./.intellishell) file in your workspace's root
  directory and commit it to git. These commands are temporary, prioritized in search results, and don't clutter your
  global library. It's the perfect way to define and share common tasks (like build or deploy scripts) with your team.

- **Share your knowledge**: Found a set of commands that could help others? Use the export feature to a public Gist of
  your bookmarks. Share the link on your blog, with your team, or contribute to a curated list
  ([example](https://gist.github.com/lasantosr/137846d029efcc59468ff2c9d2098b4f)).

- **Import everything**: Use the AI-powered import to extract command templates from virtually any text. Point it at a
  blog post, a cheatsheet, or even your own shell history to turn useful examples into reusable commands.

- **Name your variables wisely**: You have full control over which suggestions are shared between commands. Suggestions
  are grouped by variable name and root cmd. Use the same name to share suggestions, or different names to keep them separate.
  - **Shared**: Using `{{image}}` for both `docker run` and `docker rmi` will share the same list of image suggestions.
  - **Separate**: Keep suggestions for different use cases distinct by using different variable names. For example, you
    can separate your `ssh` connections for `{{prod_server}}` and `{{staging_server}}` environments. Giving each a distinct
    alias like `sshp` and `sshs` lets you quickly access the right set of servers.

## IntelliShell vs. Shell History

A common question is: "How is this different from my shell's history (`ctrl+r`) or enhanced history tools like [Atuin](https://atuin.sh/)?"

The key distinction is that they solve different problems and are **complementary tools**, not competitors.

- **`ctrl+r`** (Shell History) is perfect for recalling a specific, unique command you ran recently.
  It's a chronological log of **_what you've done_**.
- **`ctrl+space`** (IntelliShell) is for your day-to-day, frequently used commands, or for discovering how to perform a
  task in a new project. It's a curated library of **_what you want to do_**.

To put it another way: your shell history is an automatic, unfiltered log of everything you've ever typed—the good, the
bad, and the typos.

IntelliShell, by contrast, is your personal, curated collection of command "recipes" that you've chosen to save,
organize, and even share.

| Aspect                 | Shell History / Atuin                                                    | IntelliShell                                                                                             |
| ---------------------- | ------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------- |
| **Primary Purpose**    | A chronological log of every command you execute                         | A curated library of reusable command _templates_                                                        |
| **Content**            | Often cluttered with raw, one-off commands, and typos                    | A clean, organized, and intentional set of useful commands                                               |
| **Repetitive Tasks**   | Stores every unique variation (e.g., `ssh host1`, `ssh host2`)           | Stores one template (`ssh {{host}}`) and remembers your past inputs for quick reuse                      |
| **Project Onboarding** | You must read `READMEs` or other docs to find project-specific commands  | Just hit `ctrl+space` in a new repo or devcontainer to instantly discover available tools and commands   |
| **Command Discovery**  | Limited to commands _you_ have personally run before                     | Discover commands from your team (`.intellishell` files), the community (Gists), or `tldr` pages         |
| **Core Philosophy**    | **Recall**: "What was that exact command I ran yesterday?"               | **Intent**: "How do I perform this common task?"                                                         |

By using both, you get the best of both worlds: a comprehensive log for forensic searches and a powerful, personal, and
collaborative knowledge base for everything else.

## License

IntelliShell is licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for the full license text.
