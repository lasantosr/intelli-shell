# Workspace File

While your global command library is perfect for personal, frequently used commands, many tasks are specific to a
particular workspace or repository. IntelliShell addresses this with `.intellishell` files, a simple way to define and
share commands and completions that are relevant only to your current working directory.

## How It Works

When you trigger a search, IntelliShell automatically searches for `.intellishell` files using a built-in hierarchy:

1. **Local workspace**: Searches upward from the current directory until reaching a `.git` directory or filesystem root
2. **Home directory**: `~/.intellishell` (file or directory)
3. **System-wide**: `/etc/.intellishell` (Unix) or `C:\ProgramData\.intellishell` (Windows)

Each location can be either a file or directory:
- **File**: Loaded with parent directory name as tag
- **Directory**: All files inside are loaded recursively with file name as tag (hidden files are skipped)

All found files are loaded into a temporary, session-only library. These commands are given top priority in search results, appearing above your personal and `tldr` commands. Duplicate files (based on path) are automatically skipped.

> **Note**: You can temporarily disable this feature by setting the `INTELLI_SKIP_WORKSPACE=1` environment variable. If
> this variable is set, IntelliShell will not search for or load any `.intellishell` file.

### Key Behaviors

- **Workspace-Aware**: Commands and completions are only available when you are working inside that workspace's directory
  tree
- **Session-Only**: Workspace items are not saved to your permanent database, they are loaded fresh for each session
- **Top Priority**: Workspace-specific commands always appear at the top of your search results, making them easy to find
- **Version Controllable**: Since it's a plain text file, you can commit `.intellishell` to your Git repository to share
  common workspace commands with your entire team

## Use Cases

Workspace-specific commands and completions are ideal for:

- **Team Onboarding**: New team members can instantly access common build, test, and deployment commands
- **Complex Workflows**: Document and share multi-step processes, like database migrations or release procedures
- **Discoverability**: Make workspace-specific scripts and tools easily discoverable without needing to `ls` through a
  `scripts/` directory, just hit <kbd>Ctrl</kbd>+<kbd>Space</kbd> to discover them

## File Format

The `.intellishell` file uses the same simple text format as the `import` and `export` commands. Any commented line (`#`)
before a command is treated as its description, you can check IntelliShell's [own file](https://github.com/lasantosr/intelli-shell/blob/main/.intellishell)
on the repo.

Here is an example `.intellishell` file for a terraform repo:

```sh
#!intelli-shell

# Format all Terraform files in the project
terraform fmt -recursive

# [alias:tfp] Plan infrastructure changes for a specific environment
terraform plan -var-file="envs/{{env}}.tfvars"

# [alias:tfa] Apply infrastructure changes for a specific environment
terraform apply -var-file="envs/{{env}}.tfvars"

# Show the state for a specific resource
terraform state show '{{resource}}'

## -- Completions --
$ (terraform) env: find envs -type f -name "*.tfvars" -printf "%P\n" | sort | sed 's/\.tfvars$//'
$ (terraform) resource: terraform state list
```

---

Between your personal library and the workspace-specific file, you have a powerful system for managing commands and completions.
Let's now see how you can back up, restore, and share your personal commands across different machines in
[**Syncing and Sharing**](./syncing.md).
