# tldr clear

The `tldr clear` command removes command examples that were previously imported from `tldr` pages. This allows you to
clean up your library or remove categories you no longer need.

This operation only affects `tldr` commands and will not touch your personal, user-created bookmarks.

## Usage

```sh
intelli-shell tldr clear [CATEGORY]
```

## Arguments

- **`[CATEGORY]`**: Specifies a `tldr` category to clear from your local library. If this argument is omitted, all
  `tldr` commands from all categories will be removed.

  For a list of official categories, see the [tldr pages repository](https://github.com/tldr-pages/tldr/tree/main/pages).

## Examples

- **Clear all imported tldr commands**:
  This is useful if you want to start fresh or remove all `tldr` examples from your search results.

  ```sh
  intelli-shell tldr clear
  ```

- **Clear a specific category**:
  If you've previously fetched pages for a specific platform or category (e.g., `osx`) and no longer need them, you can
  remove them individually.

  ```sh
  # Remove only the macOS-specific commands
  intelli-shell tldr clear osx
  ```
