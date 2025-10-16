# Key Concepts

Before diving into practical examples, let's cover the core ideas that make this tool a powerful addition to your
command-line workflow.

## The IntelliShell Philosophy: A Library of Intents

IntelliShell is designed to transform your command-line from a simple execution tool into a structured, intelligent
library. It's built around the idea of capturing your _intent_—the tasks you want to accomplish—and turning them into
reusable, interactive "recipes".

This approach helps you work more efficiently by:

- **Building a Structured Library**: Organize your most-used commands into a searchable knowledge base with
  descriptions and tags, making them easy to find and understand.

- **Streamlining Complex Workflows**: Instead of running one command to find an ID and then manually typing it into a
  second command, IntelliShell lets you build dynamic commands that fetch and use this information in a single, fluid
  step.

- **Reducing Repetitive Typing**: Save yourself from re-typing long, complex commands. A short search or alias is all
  you need to bring up the exact template you're looking for.

- **Minimizing Typos**: By reusing tested and saved command templates, you significantly reduce the chance of making
  small, frustrating typos that lead to errors.

Ultimately, IntelliShell helps you focus on _what_ you want to do, rather than the tedious details of _how_ you have to
do it.

## Your Command-Line Companion

IntelliShell is designed to be an integral part of your daily shell workflow. Instead of manually typing
`intelli-shell` commands, you'll primarily interact with it through a set of convenient hotkeys directly from your
command line.

- **Search** <kbd>Ctrl</kbd>+<kbd>Space</kbd>: Opens an interactive search UI to find your bookmarked commands
- **Bookmark** <kbd>Ctrl</kbd>+<kbd>B</kbd>: Opens a UI to save the command currently on your command line
- **Fix Command** <kbd>Ctrl</kbd>+<kbd>X</kbd>: Uses AI to analyze and suggest a fix for a failed command
- **Variable Replace** <kbd>Ctrl</kbd>+<kbd>L</kbd>: Opens the variable replacement UI for the command
- **Clear Line** <kbd>Esc</kbd>: A convenience hotkey to clear the entire command line

> 💡 **Note**: These shell hotkeys are fully customizable. See the [**Installation**](./installation.md#customizing-keybindings)
> chapter for details on how to change them.

## The Building Blocks

IntelliShell's power comes from combining three simple but powerful concepts:

### 1. Command Templates

The fundamental building block in IntelliShell is the **command template**. Think of it as a smart snippet or a reusable
"recipe" for a command-line task. It represents a generic version of a command that you can adapt to different
situations by filling in specific details when you use it.

### 2. Variables and Suggestions

Templates become truly powerful with **variables**. Any part of a command enclosed in _`{{...}}`_ becomes a
placeholder. When you select a template, IntelliShell's UI will prompt you to fill in a value for each variable.

Best of all, IntelliShell remembers the values you use. The next time you use the same template, your previous entries
will be suggested, saving you from re-typing common inputs.

### 3. Dynamic Completions

For variables with values that change frequently, you can define **dynamic completions**. A completion is a shell
command that runs in the background to generate a list of suggestions on the fly. This is perfect for things like:

- Listing available Git branches for a `git checkout {{branch}}` command
- Showing running Docker containers for `docker exec -it {{container}} bash`
- Fetching Kubernetes services for `kubectl logs {{service}}`

---

Now that you understand the core concepts, let's walk through [**Your First Session**](./your_first_session.md).
