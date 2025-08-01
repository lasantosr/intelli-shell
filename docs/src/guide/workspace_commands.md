# Workspace-Specific Commands

While your global command library is perfect for personal, frequently used commands, many tasks are specific to a
particular workspace or repository. IntelliShell addresses this with `.intellishell` files, a simple way to define and
share commands that are relevant only to your current working directory.

## How It Works

When you trigger a search, IntelliShell automatically looks for a file named `.intellishell` in the current directory. If
it doesn't find one, it searches parent directories until it reaches a `.git` directory or the filesystem root.

If an `.intellishell` file is found, its commands are loaded into a temporary, session-only library. These commands are
given top priority in search results, appearing above your personal and `tldr` commands.

> **Note**: You can temporarily disable this feature by setting the `INTELLI_SKIP_WORKSPACE=1` environment variable. If
> this variable is set, IntelliShell will not search for or load any `.intellishell` file.

### Key Behaviors

- **Context-Aware**: Commands are only available when you are working inside that workspace's directory tree
- **Session-Only**: Workspace commands are not saved to your permanent database. They are loaded fresh for each session
- **Top Priority**: Workspace-specific commands always appear at the top of your search results, making them easy to find
- **Version Controllable**: Since it's a plain text file, you can commit `.intellishell` to your Git repository to share
  common workspace commands with your entire team

## Use Cases

Workspace-specific commands are ideal for:

- **Team Onboarding**: New team members can instantly access common build, test, and deployment commands
- **Complex Workflows**: Document and share multi-step processes, like database migrations or release procedures
- **Discoverability**: Make workspace-specific scripts and tools easily discoverable without needing to `ls` through a
  `scripts/` directory, just hit <kbd>Ctrl</kbd>+<kbd>Space</kbd> to discover them

## File Format

The `.intellishell` file uses the same simple text format as the `import` and `export` commands. Any commented line (`#`)
before a command is treated as its description.

Here is an example `.intellishell` file for a web project:

```sh
#!intelli-shell

# Run the development server
npm run dev

# Build the project for production #build
npm run build

# Run all linters and formatters #lint #test
npm run lint && npm run format

# Run end-to-end tests with playwright #test
npx playwright test
```

Between your personal library and workspace-specific files, you have a powerful system for managing commands. Let's now
see how you can back up, restore, and share your personal commands across different machines in
[**Syncing and Sharing**](./syncing.md).
