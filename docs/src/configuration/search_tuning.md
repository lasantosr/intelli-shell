# Search Tuning

IntelliShell provides advanced control over its search-ranking algorithms, allowing you to fine-tune how commands and
variable suggestions are sorted to better match your personal workflow. These settings are located in the
`[tuning.commands]` and `[tuning.variables]` sections of your `config.toml` file.

The ranking system for both commands and variables works by assigning points from different sources (like text
relevance or usage history) and calculating a final weighted score. By adjusting the points allocated to each source,
you can influence which factors are more important in the search results.

## Command Search Tuning

The final score for a searched command is a weighted sum of points from three sources: how well the text matches the
query, how often the command has been used, and the directory where it was last used.

```toml
{{#include ../../../default_config.toml:184:217}}
```

### Command Scoring Parameters

| Key                    | Description                                                                                     |
| ---------------------- | ----------------------------------------------------------------------------------------------- |
| `usage.points`         | Total points assigned based on the command's global usage count                                 |
| `path.points`          | Total points assigned based on the command's usage history in relation to the current directory |
| `path.exact`           | Multiplier for a command used in the exact same directory                                       |
| `path.ancestor`        | Multiplier for a command used in a parent directory                                             |
| `path.descendant`      | Multiplier for a command used in a child directory                                              |
| `path.unrelated`       | Multiplier for a command used in an unrelated directory                                         |
| `text.points`          | Total points assigned based on how well the command's text matches the search query             |
| `text.command`         | The weight given to matches within the command string itself (e.g., `docker run...`)            |
| `text.description`     | The weight given to matches within the command's description and hashtags                       |

### "Auto" Mode Specific Tuning

These settings only apply when the search `mode` is set to `"auto"`. They control how much weight is given to different
kinds of text matches to produce more intuitive results.

| Key                 | Description                                                                                  |
| ------------------- | -------------------------------------------------------------------------------------------- |
| `text.auto.prefix`  | A multiplier for high-confidence results where the query is a prefix of the command or alias |
| `text.auto.fuzzy`   | A multiplier for standard fuzzy-matched results where all words in the query are found       |
| `text.auto.relaxed` | A multiplier for lower-confidence results where only some words in the query are found       |
| `text.auto.root`    | A boost applied when the first search term matches the very beginning of a command string    |

## Variable Suggestion Tuning

When you replace variables in a command, IntelliShell suggests previously used values. The ranking of these suggestions
is determined by a score calculated from two sources: the context of other variables in the command and the path where
the value was used. Total usage count is used as a tie-breaker.

```toml
{{#include ../../../default_config.toml:162:182}}
```

### Variable Scoring Parameters

| Key                 | Description                                                                                   |
| ------------------- | --------------------------------------------------------------------------------------------- |
| `completion.points` | Total points assigned for being present on dynamic variable completions                       |
| `context.points`    | Total points assigned for matching the context (i.e., other variable values already selected) |
| `path.points`       | Total points assigned based on the value's usage history relative to the current directory    |
| `path.exact`        | Multiplier for a value used in the exact same directory                                       |
| `path.ancestor`     | Multiplier for a value used in a parent directory                                             |
| `path.descendant`   | Multiplier for a value used in a child directory                                              |
| `path.unrelated`    | Multiplier for a value used in an unrelated directory                                         |

---

Now that you have the search ranking system fine-tuned to your workflow, you can enhance your productivity even further
by leveraging artificial intelligence. Let's explore how to set up [**AI Integration**](./ai.md).
