# Basic Usage

IntelliShell is designed to be an integral part of your daily shell workflow. Instead of manually typing `intelli-shell`
commands, you'll primarily interact with it through a set of convenient hotkeys directly from your command line.

The core idea is simple: type a command, bookmark it, and then quickly find and reuse it later—all without leaving your
terminal prompt.

## Core Hotkeys

By default, IntelliShell sets up several hotkeys. These are the primary ways you will interact with the tool.

- **Search** <kbd>Ctrl</kbd>+<kbd>Space</kbd>: This is your main entry point. It opens an interactive search UI to find
  your bookmarked commands. If you have text on the command line, it will be used as the initial search query.

- **Bookmark** <kbd>Ctrl</kbd>+<kbd>B</kbd>: When you've typed a command you want to save, this key opens a UI to
  bookmark it. The current text on your command line will be pre-filled as the command to be saved.

- **Fix Command** <kbd>Ctrl</kbd>+<kbd>X</kbd>: When a command fails, press the up arrow to recall it, then use this
  key to let AI analyze the command and the error message to suggest a working version.

- **Variable Replace** <kbd>Ctrl</kbd>+<kbd>L</kbd>: If the command on your line contains `{{variables}}`, this key
  opens the variable replacement UI to fill them in without needing to save the command first.

- **Clear Line** <kbd>Esc</kbd>: As a convenience, this key is bound to clear the entire command line. This can be
  disabled if it conflicts with your existing terminal habits.

> 📝 **Note**: These shell hotkeys are fully customizable, see the [**Installation**](./installation.md#customizing-shell-integration)
> chapter for details on how to change them.
>
> Additionally, all keybindings used within the interactive UIs mentioned on the book are customizable in your
> configuration file, you can update them on the [**Key Bindings**](../configuration/keybindings.md) chapter.

## Your First Bookmark

Let's walk through the fundamental workflow: saving a command, searching for it, and running it again with new values.

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

   IntelliShell places the final, ready-to-run command onto your shell prompt:

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

## AI-Powered Workflows

Beyond managing bookmarks, IntelliShell's can act as your command-line co-pilot, helping you fix errors and generate
new commands when you're stuck.

### Fix a Failing Command

We've all been there: you type a long command, only for it to fail due to a small typo. Instead of manually debugging,
let the AI handle it.

1. **Run a command that fails**

   Let's try to list files with a mistyped flag:

   ```sh
   ls --long
   ```

   Your shell will return an error: `ls: unrecognized option '--long'`.

2. **Recall and Fix**

   - Press the <kbd>Up Arrow</kbd> key in your shell to bring back the failing command
   - With `ls --long` on the line, press <kbd>Ctrl</kbd>+<kbd>X</kbd>

   The AI will analyze both the command and the error it produced. It will suggest the corrected command (`ls -l` or
   `ls --format=long`), placing it directly on your prompt.

### Generate a Command from a Description

Can't remember the exact syntax for `find` or `ffmpeg`? Just describe what you want to do.

1. **Open the search UI**

   Press <kbd>Ctrl</kbd>+<kbd>Space</kbd> to open the IntelliShell search prompt.

2. **Describe the task**

   Instead of searching for a bookmark, type a description of the command you need. For example:
   `find all files larger than 10MB in the current folder`

3. **Generate with AI**

   With your description in the search box, press <kbd>Ctrl</kbd>+<kbd>I</kbd>. The AI will generate the corresponding
   shell command (e.g., `find . -type f -size +10M`) and show it in the results.

> 💡 **Tip**: Commands generated from the search prompt are for one-time use and are not saved automatically. If you
> want to save a command after generating it this way, you can place it on your terminal and then use the bookmark
> hotkey (<kbd>Ctrl</kbd>+<kbd>B</kbd>).
>
> For a more direct workflow, use the AI directly in the bookmarking UI. Press <kbd>Ctrl</kbd>+<kbd>B</kbd>, type your
> description in the 'Command' field, and press <kbd>Ctrl</kbd>+<kbd>I</kbd> to suggest it.

## Populating Your Library

A great command library is a complete one. Here are two ways to quickly add commands without bookmarking them one by one.

### From `tldr` Pages

If you're getting started or need a quick example for a new tool, you can populate IntelliShell with commands from the
community-driven [tldr pages](https://github.com/tldr-pages/tldr) project.

```sh
intelli-shell tldr fetch -c tar -c git
```

This will import useful command templates into a separate `tldr` space, which you can choose to include or exclude
from your searches.

> 💡 **Tip**: The `fetch` command is highly configurable, allowing you to import pages for specific commands or
> categories. For a full list of options, see the [**`fetch` command reference**](../reference/tldr_fetch.md).

Now you can type `tar` into the command line and press <kbd>Ctrl</kbd>+<kbd>Space</kbd>

- You'll see a list of `tldr` examples for `tar`
- Find the one for listing contents, select it, fill in the `{{path/to/file.tar}}` variable, and run it

### From Any Text

You can use the AI to extract and convert commands from any piece of text—a blog post, a tutorial, or even your own
shell history file.

```sh
# Import command templates from your own history
intelli-shell import -i --ai --history bash

# Or from a URL
intelli-shell import -i --ai "https://my-favorite-cheatsheet.com"
```

The AI will parse the content, identify potential commands, and convert them into reusable templates with
`{{variables}}`, ready for you to use.
For a full list of options, see the [**`import` command reference**](../reference/import.md).

---

Now that you've mastered the basics, let's dive deeper into how to use variables effectively in the next chapter:
[**Using Variables**](./variables.md).
