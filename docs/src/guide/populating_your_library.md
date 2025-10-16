# Populating Your Library

A great command library is a complete one. While you can build your collection over time by bookmarking commands as you
work, IntelliShell provides several ways to quickly populate your library from existing sources. This chapter covers the
different methods you can use to add commands, from a single bookmark to importing hundreds of examples at once.

## Manually

The most direct way to add commands is by creating them yourself. This can be done on the fly as you work or by preparing
a file for bulk import.

- **One Command at a Time**

  The most straightforward method is to bookmark commands as you use them. As you saw in the previous chapter, just type
  a command in your terminal and press <kbd>Ctrl</kbd>+<kbd>B</kbd> to save it.

- **From a Local File**

  For adding multiple commands at once, you can create a plain text file. This is perfect for preparing a set of
  commands for a new project or for your initial library setup. The file uses a simple format where comments (`#`)
  before a command are treated as its description and completions starts with `$`.

  For example, you could create a file named `my_commands.txt`:

  ```sh
  # List all running #docker containers
  docker ps

  # [alias:dlogs] Tail the logs of a #docker container
  docker logs -f {{container}}

  # --- Completions: $ (command) variable: provider
  $ (docker) container: docker ps -a --format '{{.Names}}'
  ```

  Once your file is ready, you can import all the commands and completions in one go:

  ```sh
  intelli-shell import my_commands.txt
  ```

  > 💡 **Tip**: For more details on the file format and advanced import/export options, see the
  > [**Syncing and Sharing**](./syncing_and_sharing.md) chapter.

## From `tldr` Pages

If you're getting started or need a quick set of examples for common tools, you can populate your library with commands
from the community-driven [tldr pages](https://github.com/tldr-pages/tldr) project.

```sh
intelli-shell tldr fetch -c tar -c git
```

This will import useful command templates into a separate `tldr` space, which you can choose to include or exclude
from your searches. Once fetched, they will appear in your search results, giving you instant access to a massive
library of commands.

> 💡 **Tip**: The `fetch` command is highly configurable, allowing you to import pages for specific commands or
> categories. For a full list of options, see the [**`fetch` command reference**](../reference/tldr_fetch.md).

## From a Community Gist

You can tap into the collective knowledge of the community by importing command collections from [public Gists](https://gist.github.com/search?q=intellishell+commands).
This is a great way to share and discover workflows for specific tools.

```sh
# Interactively import all commands from a Gist, allowing to discard or edit before importing
intelli-shell import -i https://gist.github.com/lasantosr/137846d029efcc59468ff2c9d2098b4f

# Or non-interactively from a specific file within a Gist
intelli-shell import --gist 137846d029efcc59468ff2c9d2098b4f/docker.sh
```

> 💡 **Tip**: The `import` command also allows you to filter commands or preview them interactively before importing.
> For a full list of options, see the [**`import` command reference**](../reference/import.md).

## From Any Text with AI

If you've enabled AI integration, you can use the `import` command to extract and convert commands from almost any piece
of text—a blog post, a tutorial, or even your own shell history file.

- **From Your Shell History**

  This is a powerful way to convert your most-used historical commands into a permanent, searchable library. The `-i`
  (interactive) flag is highly recommended to curate the results.

  ```sh
  intelli-shell import -i --ai --history bash
  ```

- **From a Website**

  Turn any online cheatsheet or tutorial into a source of ready-to-use command templates. The AI will parse the page
  and extract commands for you to review and import.

  ```sh
  intelli-shell import -i --ai "https://my-favorite-cheatsheet.com"
  ```

---

With a well-populated library, you're ready to master IntelliShell's most powerful feature. Let's dive into
[**Using Variables and Completions**](./variables_and_completions.md).
