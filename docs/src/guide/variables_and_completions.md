# Using Variables and Completions

As we touched on in the _Key Concepts_ chapter, variables are the core of IntelliShell's power, allowing you to turn
static commands into reusable templates. This chapter dives deeper into the advanced features of variables and shows you
how to supercharge them with dynamic completions.

## Advanced Variable Usage

The basic _`{{variable}}`_ syntax is straightforward, but you can add more intelligence and control to your templates.

> **Note**: While _`{{...}}`_ is the primary syntax for variables, _`<...>`_ is also accepted for compatibility when
> replacing labels or importing commands from other systems.

### Secret Variables

Some variables, like comments, API tokens or passwords, shouldn't be saved in your history.
You can mark a variable as secret by enclosing it in an extra set of curly braces: _`{{{secret-variable}}}`_

```sh
curl -u user:{{{password}}} https://api.example.com/data
```

When you fill in a secret variable:

- The value is used in the command just once.
- It is **never** saved to your suggestion history.
- If a matching environment variable (e.g., `PASSWORD`) exists, IntelliShell will suggest using the variable itself
  (e.g., `$PASSWORD` or `$env:PASSWORD`) in the final command, preventing the secret value from ever being exposed in
  plain text.

### Providing Default Suggestions

You can provide a list of predefined options for a variable directly in its definition using the pipe (`|`) character.
These options will appear as initial suggestions in the UI. This is perfect for commands where the input is one of a few
known values.

```sh
# Provides 'up', 'down', and 'logs' as initial suggestions
docker-compose {{up|down|logs}}
```

### Formatting Input with Functions

IntelliShell can automatically format the text you enter for a variable before inserting it into the final command.
Functions are appended to the variable name, separated by colons (`:`).

**Syntax**: _`{{variable_name:function1:function2}}`_

For example, a common task is creating a git-friendly branch name from a description. You can automate the formatting:

```sh
# Input: "My New Feature" -> Output: "my-new-feature"
git checkout -b feature/{{{description:kebab}}}
```

Here are the available functions:

| Function | Description                   | Example Input | Example Output  |
| :------- | :---------------------------- | :------------ | :-------------- |
| `kebab`  | Converts text to `kebab-case` | `My Project`  | `My-Project`    |
| `snake`  | Converts text to `snake_case` | `My Project`  | `My_Project`    |
| `upper`  | Converts text to `UPPERCASE`  | `hello`       | `HELLO`         |
| `lower`  | Converts text to `lowercase`  | `HELLO`       | `hello`         |
| `url`    | URL-encodes the text          | `a/b?c=1`     | `a%2Fb%3Fc%3D1` |

Functions are applied from left to right.

### How Suggestions Are Managed

IntelliShell is smart about how it stores and suggests values for your variables. Suggestions are shared based on two
factors:

1. **The Root Command**: The first word of your command (e.g., `git`, `docker`).
2. **The Variable's Identity**: The text inside the `{{...}}` braces, excluding any formatting functions.

This means:

- **Shared Suggestions**: Using `{{image}}` for both `docker run` and `docker rmi` will use the **same** list of image
  suggestions, which is usually what you want. The same applies if you use `{{image|container}}` and `{{image}}`;
  suggestions for `image` will be shared.

- **Separate Suggestions**: To keep suggestions separate, use different variable names. For example, you can have two SSH
  commands with different suggestion lists by using `ssh {{prod_server}}` and `ssh {{staging_server}}`.

This system gives you fine-grained control over which commands share suggestion histories, helping you keep different
contexts neatly organized.

### Navigating Between Variables

When filling in variables on the variable replacement TUI, you can navigate between them freely:

- <kbd>Ctrl</kbd>+<kbd>Tab</kbd>: Move to next variable. After the last variable, cycles back to the first.
- <kbd>Shift</kbd>+<kbd>Tab</kbd>: Move to previous variable. From the first variable, cycles to the last.
- <kbd>Enter</kbd>: Confirm current value and move to next variable. After the last variable, exits and executes / outputs
  the command if there are no pending variables.

Variables remember their values as you navigate. When you return to a variable you've already filled, its current value
will be pre-selected in the suggestions list, making it easy to review or change values before executing the command.

> 💡 **Tip**: You can also undo / redo the variable selection with <kbd>Ctrl</kbd>+<kbd>Z</kbd> / <kbd>Ctrl</kbd>+<kbd>Y</kbd>.

## Dynamic Completions

While providing static options with `|` is useful, the real power of templates comes from **dynamic completions**.
A completion is a shell command that IntelliShell executes to generate a list of suggestions in real-time.

> 💡 **Important**: Completions run asynchronously in the background. When IntelliShell prompts for a variable, it
> instantly shows suggestions from your history. The dynamic suggestions are merged in as soon as they are ready, so
> your workflow is never blocked, even by slow network commands.

### Global vs. Command-Specific Completions

You can define completions to be either global or tied to a specific command.

- **Command-Specific (Recommended)**: This is the most common use case. The completion only runs when the variable
  appears in a command starting with a specific root command (e.g., `git`).

  ```sh
  # This completion runs only for 'git' commands with a {{branch}} variable
  intelli-shell completion new --command git branch "git branch --format='%(refname:short)'"
  ```

- **Global**: By omitting the `--command` flag, you create a global completion that applies to a variable name in _any_
  command. This is useful for universally applicable variables.

  ```sh
  # A global completion for {{user}}, useful for commands like `chown {{user}} ...`
  intelli-shell completion new user "awk -F: '\$3 >= 1000 {print \$1}' /etc/passwd"
  ```

### Context-Aware Completions

Completions can adapt their suggestions based on the values of other variables you've already filled in. This is done by
using **conditional variables** inside the completion's command itself. IntelliShell will substitute these variables
before execution, making your completions context-aware.

#### Example: Contextual Kubernetes Pods

Imagine you have a command to view logs for a pod in a specific namespace: `kubectl logs -n {{namespace}} {{pod}}`.
You want the `{{pod}}` suggestions to be filtered by the `{{namespace}}` you just selected.
You can achieve this with two completions:

1. **Namespace Completion**: First, create a completion to list all available namespaces.

    ```sh
    intelli-shell completion new --command kubectl namespace "kubectl get ns --no-headers -o custom-columns=':.metadata.name'"
    ```

2. **Context-Aware Pod Completion**: Next, create a completion for pods that uses the `{{namespace}}` variable within its
    own command.

    ```sh
    intelli-shell completion new --command kubectl pod "kubectl get pods {{-n {{namespace}}}} --no-headers -o custom-columns=':.metadata.name'"
    ```

Now, when you use the `kubectl logs` template, IntelliShell first prompts for the `namespace`. Once you select one, it
substitutes that value into the pod completion's command, running `kubectl get pods -n <selected-namespace> ...` to get a
list of pods only from that specific namespace.

---

Now that you can create powerful, reusable command templates, let's look at how to manage commands that are specific to
your current project in [**Workspace Library**](./workspace_library.md).
