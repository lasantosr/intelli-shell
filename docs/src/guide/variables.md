# Using Variables

The true power of IntelliShell lies in its ability to create reusable command templates. Instead of saving a static
command like `docker run ubuntu:latest`, you can create a dynamic version: `docker run {{image}}`.

We call these dynamic placeholders **variables**. When you execute a command with variables, IntelliShell will prompt you
to provide values for them on the fly, turning a specific command into a versatile, reusable tool.

## Basic Syntax

A variable is any text enclosed in double curly braces: `{{variable_name}}`.

```sh
echo "Hello, {{user}}!"
```

When you use this command, IntelliShell's variable replacement UI will open, prompting you for a value for the `user`
variable. The value you provide will be stored as a suggestion for the next time you use a `{{user}}` variable in an
`echo` command.

> **Note**: While `{{...}}` is the primary syntax for variables, `<...>` is also accepted for compatibility when
> replacing labels or importing commands from other systems.

## Secret Variables

Some variables, like comments, API tokens or passwords, shouldn't be saved in your suggestion history. You can mark a
variable as secret by enclosing it in an extra set of curly braces: `{{{secret_variable}}}`.

```sh
curl -u user:{{{password}}} https://api.example.com/data
```

When you fill in a secret variable:

- The value is used in the command just once
- It is **never** saved to your suggestion history
- If a matching environment variable (e.g., `PASSWORD`) exists, IntelliShell will suggest using the variable itself
  (e.g., `$PASSWORD` or `$env:PASSWORD`) in the final command, preventing the secret value from ever being exposed in
  plain text

## Providing Default Suggestions

You can provide a list of predefined options for a variable directly in its definition using the pipe (`|`) character.
These options will appear as "derived" suggestions in the UI.

This is perfect for commands where the input is one of a few known values, like a subcommand.

```sh
# Provides 'up', 'down', and 'logs' as initial suggestions
docker-compose {{up|down|logs}}
```

## Formatting Input with Functions

IntelliShell can automatically format the text you enter for a variable before inserting it into the final command.
Functions are appended to the variable name, separated by colons (`:`).

**Syntax**: `{{variable_name:function1:function2}}`

For example, a common task is creating a git branch name from a description. You can automate the formatting:

```sh
# Input: "My New Feature" -> Output: "feature/my-new-feature"
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

## How Suggestions Are Managed

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

## Navigating Between Variables

When filling in variables on the variable replacement TUI, you can navigate between them freely:

- <kbd>Alt</kbd>+<kbd>J</kbd>: Confirm current value and move to next variable. After the last variable, cycles back to
  the first.
- <kbd>Alt</kbd>+<kbd>K</kbd>: Move to previous variable. From the first variable, cycles to the last.
- <kbd>Enter</kbd>: Confirm current value and move to next variable. After the last variable, exits and executes / outputs
  the command.

Variables remember their values as you navigate. When you return to a variable you've already filled, its current value
will be pre-selected in the suggestions list, making it easy to review or change values before executing the command.

> 💡 **Tip**: You can also undo / redo the variable selection with <kbd>Ctrl</kbd>+<kbd>Z</kbd> / <kbd>Ctrl</kbd>+<kbd>Y</kbd>.

## Dynamic Suggestions with Completions

While providing static options with `|` is useful, the real power of templates comes from **dynamic completions**.
A completion is a shell command that IntelliShell executes to generate a list of suggestions in real-time.

To ensure your workflow is never interrupted, completions run **asynchronously in the background**. When IntelliShell
prompts for a variable that has an associated completion:

- **Instantly**, you'll see suggestions from your history and matching environment variables.
- **In the background**, the completion command is executed.
- **Once finished**, its output (split by newlines) is seamlessly merged into the existing suggestion list.

This non-blocking approach means you can include potentially slow network commands (like `kubectl` or `gh`) as
completions without ever slowing down your terminal experience.

For instance, to get dynamic suggestions for all local git branches whenever you use a `{{branch}}` variable in a `git`
command, you can register the following completion:

```sh
intelli-shell completion new --command git branch "git branch --format='%(refname:short)'"
```

Now, commands like `git checkout {{branch}}` or `git rebase {{branch}}` will automatically suggest your local branches,
making your workflow faster and less error-prone.

### Global Completions

By omitting the `--command` flag, you create a **global completion** that applies to a variable name in _any_ command.
This is ideal for universally useful variables, like system usernames.

```sh
# A global completion for `{{user}}`, useful for commands like `chown {{user}} ...`
intelli-shell completion new user "awk -F: '\$3 >= 1000 {print \$1}' /etc/passwd"
```

It's important to note that IntelliShell always prioritizes specificity. If a command-scoped completion also exists for the
same variable, it will **always take precedence** over the global one.

### Context-Aware Completions

Completions can adapt their suggestions based on the values of other variables you've already filled in. This is done by
using variables inside the completion's command itself. IntelliShell will substitute these variables before execution,
making your completions context-aware.

#### Example: Contextual Kubernetes Pods

Imagine you have a command to view logs for a pod in a specific namespace: `kubectl logs -n {{namespace}} {{pod}}`.
You want the `{{pod}}` suggestions to be filtered by the `{{namespace}}` you just selected.

You can achieve this with two completions:

1. **Namespace Completion**: First, create a completion to list all available namespaces.

   ```sh
   intelli-shell completion new --command kubectl namespace "kubectl get ns --no-headers -o custom-columns=':.metadata.name'"
   ```

2. **Context-Aware Pod Completion**: Next, create a completion for pods that uses the `{{namespace}}` variable.

   ```sh
   intelli-shell completion new --command kubectl pod "kubectl get pods {{-n {{namespace}}}} --no-headers -o custom-columns=':.metadata.name'"
   ```

Now, when you use the `kubectl logs` template, IntelliShell will first prompt for the `namespace`. Once you select one
(e.g., `production`), it substitutes that value into the pod completion's command, running
`kubectl get pods -n production ...` to get a list of pods only from that specific namespace. Because this runs in the
background, you can immediately type a pod name you already know without waiting for the `kubectl` command to return its
list.

---

Now that you can create powerful, reusable command templates, let's look at how to manage commands that are specific to
your current workspace in [**Workspace File**](./workspace.md).
