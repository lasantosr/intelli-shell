# Key Bindings

Key bindings in IntelliShell are a mix of high-level **customizable actions** and built-in **standard controls**. This
approach provides flexibility for core actions while maintaining a consistent, hardcoded set of keys for navigation and
text editing, many of which follow common terminal and Emacs-like conventions.

> ⚠️ **Terminal Compatibility**
>
> Your ability to use certain key combinations depends entirely on your terminal emulator. Some terminals may capture
> specific hotkeys (like `ctrl-space` or `ctrl-enter`) for their own features and will not forward them to
> IntelliShell. This is particularly common on Windows with terminals like Cmder or older versions of Windows Terminal.
> If a key binding doesn't work, try a different combination or consult your terminal's documentation.

## Customizable Actions

These are the primary actions you can configure in the `[keybindings]` section of your `config.toml` file. They handle
the main functions of the TUI, like confirming a selection or deleting a command.

The format and a list of available actions are detailed below. Note that if a default binding for a customizable action
(like `ctrl-d` for `delete`) conflicts with a standard control, the customizable action always takes precedence.

| Action             | Description                                                           | Default Binding(s)        |
| ------------------ | --------------------------------------------------------------------- | ------------------------- |
| `quit`             | Exits the TUI gracefully without making a selection                   | `esc`                     |
| `update`           | Enters edit mode for the highlighted item (e.g., to edit a command)   | `ctrl-u`, `ctrl-e`, `F2`  |
| `delete`           | Deletes the currently highlighted item (e.g., a bookmarked command)   | `ctrl-d`                  |
| `confirm`          | Confirms a selection or moves to the next step (e.g., variable entry) | `tab`, `enter`            |
| `execute`          | Executes the highlighted command instead of just selecting it         | `ctrl-enter`, `ctrl-r`    |
| `search_mode`      | Cycles through the available search modes (auto, fuzzy, regex, etc.)  | `ctrl-s`                  |
| `search_user_only` | Toggles whether to filter user commands only in the search results    | `ctrl-o`                  |

### Default Configuration

You can change these bindings by modifying the `[keybindings]` block in your configuration file.

```toml
{{#include ../../../default_config.toml:84:107}}
```

## Standard Controls

These key bindings are **not configurable** and provide a standard way to navigate lists and edit text. They are always
active unless overridden by a customizable action.

### List & Tab Navigation

| Action                      | Key(s)               |
| --------------------------- | -------------------- |
| Move selection up           | `Up`, `ctrl-p`       |
| Move selection down         | `Down`, `ctrl-n`     |
| Navigate to previous item   | `ctrl-k`             |
| Navigate to next item       | `ctrl-j`             |

### Text Cursor Movement

| Action                      | Key(s)               |
| --------------------------- | -------------------- |
| Move to start of line       | `Home`, `ctrl-a`     |
| Move to end of line         | `End`, `ctrl-e`      |
| Move left one character     | `Left`, `ctrl-b`     |
| Move right one character    | `Right`, `ctrl-f`    |
| Move left one word          | `alt-b`, `ctrl-Left` |
| Move right one word         | `alt-f`, `ctrl-Right`|

### Text Editing

| Action                      | Key(s)                              |
| --------------------------- | ----------------------------------- |
| Delete char before cursor   | `Backspace`, `ctrl-h`               |
| Delete word before cursor   | `ctrl-Backspace`, `ctrl-w`          |
| Delete char at cursor       | `Delete`, `ctrl-d`                  |
| Delete word at cursor       | `alt-d`, `ctrl-Delete`              |
| Insert a newline            | `shift-Enter`, `alt-Enter`, `ctrl-m`|
| Undo                        | `ctrl-z`, `ctrl-u`                  |
| Redo                        | `ctrl-y`, `ctrl-r`                  |

With your keybindings configured, you can now personalize the application's appearance. Let's dive into
[**Theming**](./theming.md).
