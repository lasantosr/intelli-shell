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

| Function | Description | Example Input | Example Output |
| :--- | :--- | :--- | :--- |
| `kebab` | Converts text to `kebab-case` | `My Project` | `My-Project` |
| `snake` | Converts text to `snake_case` | `My Project` | `My_Project` |
| `upper` | Converts text to `UPPERCASE` | `hello` | `HELLO` |
| `lower` | Converts text to `lowercase` | `HELLO` | `hello` |
| `url` | URL-encodes the text | `a/b?c=1` | `a%2Fb%3Fc%3D1` |

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

---

Now that you can create powerful, reusable command templates, let's look at how to manage commands that are specific to
your current workspace in [**Workspace-Specific Commands**](./workspace_commands.md).
