# `search`

The `search` command finds stored commands based on a query.

By default, this command performs a non-interactive search and prints the matching commands directly to your terminal's
standard output. To open the interactive search TUI (the behavior typically associated with the `ctrl+space` hotkey), you
must use the `-i` or `--interactive` flag.

## Usage

```sh
intelli-shell search [OPTIONS] [QUERY]
```

## Arguments

- **`QUERY`**
  
  The search query used to filter commands.

## Options

- `-m, --mode <MODE>`
  
  Specifies the search algorithm to use, overriding the default set in your configuration file.
  See the "Advanced Search Syntax" section below for syntax specific to `auto` and `fuzzy` modes.
  - `auto`: Uses an internal algorithm to best match common search patterns.
  - `fuzzy`: Finds commands that are similar to the query using special syntax.
  - `regex`: Treats the query as a regular expression for complex pattern matching.
  - `exact`: Returns only commands that precisely match the entire query.
  - `relaxed`: Broadens the search to find the maximum number of potentially relevant commands.

- `-u, --user-only`
  
  If set, the search will exclude commands imported from `tldr` pages.

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

### 1. Open the Interactive Search

To launch the TUI, you must use the `--interactive` flag.

```sh
intelli-shell search --interactive
```

### 2. Perform a Non-Interactive Search

To search for commands matching "docker" and print them to the console:

```sh
intelli-shell search docker
```

### 3. Non-Interactive Search with Options

To find only your custom commands that exactly match "docker":

```sh
intelli-shell search -m exact --user-only docker
```

### 4. Open the Interface in Full-Screen Mode

To launch the interactive TUI and force it into full-screen mode:

```sh
intelli-shell search -i --full-screen
```
