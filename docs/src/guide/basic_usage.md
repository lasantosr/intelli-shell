# Basic Usage

IntelliShell is designed to be an integral part of your daily shell workflow. Instead of manually typing `intelli-shell`
commands, you'll primarily interact with it through a set of convenient hotkeys directly from your command line.

The core idea is simple: type a command, bookmark it, and then quickly find and reuse it later—all without leaving your
terminal prompt.

## Core Hotkeys

By default, IntelliShell sets up three main hotkeys. These are the primary ways you will interact with the tool.

- **Search** <kbd>Ctrl</kbd>+<kbd>Space</kbd>: This is your main entry point. It opens an interactive search UI to find
  your bookmarked commands. If you have text on the command line, it will be used as the initial search query.

- **Bookmark** <kbd>Ctrl</kbd>+<kbd>B</kbd>: When you've typed a command you want to save, this key opens a UI to
  bookmark it. The current text on your command line will be pre-filled as the command to be saved.

- **Variable Replace** <kbd>Ctrl</kbd>+<kbd>L</kbd>: If the command on your line contains `{{variables}}`, this key
  opens the variable replacement UI to fill them in without needing to save the command first.

- **Clear Line** <kbd>Esc</kbd>: As a convenience, this key is bound to clear the entire command line. This can be
  disabled if it conflicts with your existing terminal habits.

> 📝 **Note**: These hotkeys are fully customizable. See the [Installation](./installation.md) chapter for details on
> how to change them.

## Your First Bookmark

Let's walk through a common use case: saving a command, searching for it, and running it again with new values.

1. **Write a command**

   Type a command you find yourself using often. For this example, we'll use a `docker` command with a placeholder for
   the image name. In your terminal, type:

   ```sh
   docker run -it --rm {{image}}
   ```

2. **Bookmark it**

   With the command still on the line, press <kbd>Ctrl</kbd>+<kbd>B</kbd>. The bookmarking UI will appear. You can add
   more details here:
   - **Alias**: A short, memorable name. Let's use `dr`. This allows for quick lookups later.
   - **Description**: A brief explanation. Let's add `Run a temporary docker image #docker`.

   Press <kbd>Enter</kbd> to save the bookmark.

3. **Search for it**

   Later, when you need that command, type its alias `dr` and press <kbd>Ctrl</kbd>+<kbd>Space</kbd>.
   Because there's only one command matching the `dr` alias, the search UI is skipped, and you are taken directly to the
   variable replacement screen.
   - Type `ubuntu:latest` and press <kbd>Enter</kbd>.

   IntelliShell replaces the variable and places the final, ready-to-run command onto your shell prompt:

   ```sh
   docker run -it --rm ubuntu:latest
   ```

   Just press <kbd>Enter</kbd> one last time in your shell to execute it!

4. **Re-run with new values**

   Now if you need to run the same command, but with the `debian` image, there's no need to re-type anything.
   - Use the <kbd>Up Arrow</kbd> key in your shell to recall the last command
   - With `docker run -it --rm ubuntu:latest` on the line, press <kbd>Ctrl</kbd>+<kbd>Space</kbd>
   - IntelliShell recognizes the command's original template and shows it as the top result, so you can select it and
     provide a new value for the `{{image}}` variable

## Organize with Hashtags

Hashtags are a powerful way to categorize and quickly filter your commands. Any word in a command's description that
starts with a `#` symbol is treated as a searchable tag.

In the previous example, we added the `#docker` tag. Let's bookmark another command to see how this works.

1. **Bookmark another command**

   Type `git checkout {{branch}}` into your terminal, press <kbd>Ctrl</kbd>+<kbd>B</kbd>, and add a description with a
   hashtag like `Checkout a git branch #git`.

2. **Discover and filter by hashtag**
   Now you have commands tagged with `#docker` and `#git`. You can use these to filter your search.
   - Clear your command line, type `#`, and press <kbd>Ctrl</kbd>+<kbd>Space</kbd>
   - The search UI will suggest all the hashtags you've used, selecting one will instantly filter your command list

> 💡 **Tip**: Hashtag discovery is cumulative and considers your entire query. For example, if you search for
> `docker #compose` and then type `#` again, the suggestions will only include tags that appear on commands matching
> "docker" and already tagged with `#compose`. This lets you progressively narrow your search.

## Populating Your Library with `tldr`

If you're getting started or need a quick example for a new tool, you can populate IntelliShell with commands from the
community-driven [tldr pages](https://github.com/tldr-pages/tldr) project.

1. **Fetch `tldr` pages**

   Run the `fetch` command to download all common command examples for your operating system.

   ```sh
   intelli-shell tldr fetch
   ```

   This will import hundreds of useful command templates into a separate `tldr` space, which you can choose to
   include or exclude from your searches.

2. **Find and use a `tldr` command**

   Forgot how to list the contents of a `tar` archive?
   - Type `tar` into the command line and press <kbd>Ctrl</kbd>+<kbd>Space</kbd>
   - You'll see a list of `tldr` examples for `tar`
   - Find the one for listing contents, select it, fill in the `{{path/to/file.tar}}` variable, and run it

> 💡 **Tip**: The `fetch` command is highly configurable, allowing you to import pages for specific commands or
> categories. For a full list of options, see the [**`fetch` command reference**](../reference/tldr_fetch.md).

Now that you've mastered the basics, let's dive deeper into how to use variables effectively in the next chapter:
[**Using Variables**](./variables.md).
