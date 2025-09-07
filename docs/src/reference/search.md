# `search`

The `search` command finds stored commands based on a query and can also use AI to generate new ones on the fly.

By default, this command performs a non-interactive search and prints matching commands to standard output. To open the
interactive search TUI (the behavior of the <kbd>Ctrl</kbd>+<kbd>Space</kbd> hotkey), you must use the `-i` or
`--interactive` flag.

>💡 **Tip**: Can't find what you're looking for? While in the interactive TUI, you can press <kbd>Ctrl</kbd>+<kbd>I</kbd>
> or <kbd>Ctrl</kbd>+<kbd>X</kbd> with a natural language query to prompt AI for commands.

## Usage

```sh
intelli-shell search [OPTIONS] [QUERY]
```

## Arguments

- **`QUERY`** The search query used to filter commands.
  
  - When used with the `--ai` flag, this string is treated as a **natural language prompt** for the AI to generate a
    command.

## Options

- `-m, --mode <MODE>`

  Specifies the search algorithm to use. See the "Advanced Search Syntax" section below for details on `auto` and `fuzzy`
  modes.
  - `auto`: Uses an internal algorithm to best match common search patterns
  - `fuzzy`: Finds commands that are similar to the query using special syntax
  - `regex`: Treats the query as a regular expression for complex pattern matching
  - `exact`: Returns only commands that precisely match the entire query
  - `relaxed`: Broadens the search to find the maximum number of potentially relevant commands

- `-u, --user-only`
  
  Excludes commands imported from `tldr` pages from the search results.

- `--ai`
  
  Uses AI to generate commands based on the `QUERY` prompt instead of searching your local library. This is most
  effective in interactive mode (`-i`).

- `-i, --interactive`
  
  Opens the interactive TUI to search and select a command.

- `-l, --inline`
  
  When used with `--interactive`, forces the TUI to render inline, below the prompt.

- `-f, --full-screen`
  
  When used with `--interactive`, forces the TUI to render in full-screen mode.

## Advanced Search Syntax

The `auto` and `fuzzy` search modes support special characters to give you more control over the results.

### Auto Mode Syntax

In `auto` mode, you can exclude results containing a specific word by prefixing it with `!`.

- **Negated Term**: `!word`
  
  Excludes commands that contain `word`. For example, `docker !test` will find commands matching "docker" but not "test".

### Fuzzy Mode Syntax

`fuzzy` mode provides a powerful syntax for fine-grained matching. All terms in a query are space-separated and treated
as a logical AND, unless grouped by the `|` (OR) operator.

| Syntax       | Match Type              | Description                                                                           |
| :----------- | :---------------------- | :------------------------------------------------------------------------------------ |
| `text`       | **Fuzzy**               | Characters must appear in order, but not necessarily consecutively                    |
| `'text`      | **Exact**               | Must contain the exact substring `text`                                               |
| `'text'`     | **Word**                | Must contain `text` as a whole word                                                   |
| `^text`      | **Prefix**              | Must begin with the exact string `text`                                               |
| `text$`      | **Suffix**              | Must end with the exact string `text`                                                 |
| `!text`      | **Inverse**             | Must *not* contain the exact substring `text`                                         |
| `!^text`     | **Inverse prefix**      | Must *not* start with the exact string `text`                                         |
| `!text$`     | **Inverse suffix**      | Must *not* end with the exact string `text`                                           |
| `\|`         | **OR operator**         | A space-separated `\|` character creates a logical OR group for the terms it connects |

## Examples

### Open the Interactive Search

To launch the TUI, you must use the `--interactive` flag.

```sh
intelli-shell search --interactive
```

### Perform a Non-Interactive Search

To search for commands matching "docker" and print them to the console:

```sh
intelli-shell search docker
```

### Non-Interactive Search with Options

To find only your custom commands that exactly match "docker":

```sh
intelli-shell search -m exact --user-only docker
```

### Open the Interface in Full-Screen Mode

To launch the interactive TUI and force it into full-screen mode:

```sh
intelli-shell search -i --full-screen
```

### Use AI to Suggest Commands

To use AI to suggest commands based on a natural language prompt:

```sh
intelli-shell search -i --ai 'undo last n commits'
```

This will open the interactive interface with AI-suggested commands, which you can then review and select.

> 💡 **Tip: Saving AI-Generated Commands**
>
> Commands generated using `--ai` in the search interface are for **one-time use** and are not saved to your library
> automatically.
>
> To save a generated command for future use, you can place it on your terminal line from the search results and then
> use either the <kbd>Ctrl</kbd>+<kbd>B</kbd> hotkey or the [`new`](./new.md) command to bookmark it.
