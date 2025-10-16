# Your First Session

This chapter will walk you through a complete, hands-on workflow. You'll learn how to bookmark a command, search for it,
use variables, and even power it up with dynamic completions. Let's get started!

## 1. Bookmark Your First Command

The most fundamental workflow in IntelliShell is saving a command you use often. For this tutorial, we'll use a common
`git` command.

- First, type the following command template into your terminal. Don't press enter yet!

  ```sh
  git checkout {{branch}}
  ```

- With the command still on the line, press <kbd>Ctrl</kbd>+<kbd>B</kbd>. The bookmarking UI will appear, pre-filling the
  command you just typed.

- Now, let's add some useful details:
  - **Alias**: `gco` (a short, memorable name for quick searching)
  - **Description**: `Checkout a #git branch`

- Press <kbd>Enter</kbd> to save the bookmark. That's it! You've just saved your first command template.

## 2. Find and Use Your Command

Now, let's use the command we just saved.

- Clear your terminal and type the alias we just created:

  ```sh
  gco
  ```

- Now, press <kbd>Ctrl</kbd>+<kbd>Space</kbd>. Because there's only one command with the alias `gco`, IntelliShell skips
  the search screen and takes you directly to the variable replacement UI.

- You'll be prompted to provide a value for the `{{branch}}` variable. Type in a branch name, for example,
  `feature/new-login`, and press <kbd>Enter</kbd>.

IntelliShell now places the final, ready-to-run command onto your shell prompt:

```sh
git checkout feature/new-login
```

Just press <kbd>Enter</kbd> one last time in your shell to execute it!

## 3. Re-run with New Values

IntelliShell includes many small details designed to improve the user experience. One of these is its ability to
recognize commands you've already run—even from your shell history—and map them back to their original template.

Imagine you've just run the command above. Now you want to switch to another branch.

- Press the <kbd>Up Arrow</kbd> key in your shell to recall the last command: `git checkout feature/new-login`.
- With the full command on the line, press <kbd>Ctrl</kbd>+<kbd>Space</kbd>.

IntelliShell is smart enough to recognize that this command was generated from your `git checkout {{branch}}` template.
It will show the original template as the top result. Just press <kbd>Enter</kbd> to select it, and you'll be prompted
to enter a new value for the `{{branch}}` variable.

## 4. Organize with Hashtags

Hashtags are a great way to categorize your commands. Any word in a command's description that starts with a `#` is
treated as a searchable tag. When we created our first bookmark, we added the `#git` tag. Let's add another command to
see how this works.

- **Bookmark another command**

  Type `docker run -it --rm {{image}}` into your terminal, press <kbd>Ctrl</kbd>+<kbd>B</kbd>, and add a description with
  a hashtag like `Run a temporary #docker image`.

- **Discover and filter by hashtag**

  Now that you have commands tagged with `#git` and `#docker`, you can use these to filter your search.

  - Clear your command line (<kbd>Esc</kbd>) and press <kbd>Ctrl</kbd>+<kbd>Space</kbd>.
  - All of your bookmarked templates are displayed, type `#` to filter by a hashtag.
  - The search UI will now suggest all the hashtags you've used. Selecting one will instantly filter your command
    list to show only the commands with that tag.

> 💡 **Tip**: Hashtag discovery is cumulative and considers your entire query. For example, if you search for
> `docker #compose` and then type `#` again, the suggestions will only include tags that appear on commands matching
> "docker" and already tagged with `#compose`. This lets you progressively narrow your search.

## 5. Your First Completion

Now for the magic. Let's supercharge our `git checkout {{branch}}` command with **dynamic completions**. We'll create a
completion that automatically suggests all your local git branches.

- Run the following command in your terminal:

  ```sh
  intelli-shell completion new --command git branch "git branch --format='%(refname:short)'"
  ```

That's it! You've just told IntelliShell that whenever it sees a `{{branch}}` variable in a `git` command, it should run
`git branch --format='%(refname:short)'` in the background to fetch suggestions.

Now, try using your `gco` alias again. Press <kbd>Ctrl</kbd>+<kbd>Space</kbd>. When the variable replacement UI appears,
you'll see a list of all your local git branches, ready for you to select. No more typing branch names by hand!

---

Now that you've seen how to create and use command templates, let's explore how you can leverage artificial
intelligence to generate and fix commands for you in the [**Introduction to AI**](./introduction_to_ai.md).
