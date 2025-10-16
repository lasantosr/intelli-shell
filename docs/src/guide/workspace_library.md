# Workspace Library

While your global command library is perfect for personal, frequently used commands, many tasks are specific to a
particular workspace or repository. IntelliShell addresses this with `.intellishell` files, a simple way to define and
share commands and completions that are relevant only to your current working environment.

## How It Works

When you trigger a search, IntelliShell automatically looks for a `.intellishell` entry in the current directory.
If it doesn't find one, it searches parent directories until it reaches a `.git` directory or the filesystem root.

This location can be either a single file or a directory:

- **If it's a file**: The file is loaded, and its commands are automatically tagged with the name of the parent
  directory.
- **If it's a directory**: All files inside the directory (and its subdirectories) are loaded recursively. Each command
  is tagged with the name of the file it came from (excluding the extension). Hidden files are skipped.

All found files are loaded into a temporary, session-only library. These commands are given top priority in search
results, appearing above your personal and `tldr` commands.

> 💡 **Note**: You can temporarily disable this feature by setting the `INTELLI_SKIP_WORKSPACE=1` environment variable.

### Advanced Use Cases: User and System-Level Files

The primary and most common use case for this feature is the **local workspace file** described above.
This is the recommended approach for sharing project-specific commands with your team.

However, to support non-personal scenarios, like managing multiple machines in a network with shared folders,
IntelliShell also loads `.intellishell` files from user-level and system-wide locations. This allows you to define a
common set of commands once and have them automatically available across many systems without needing to manually import
them on each one.

- **User-Level**: `~/.intellishell`
- **System-Wide**: `/etc/.intellishell` (Unix) or `C:\ProgramData\.intellishell` (Windows)

## When to Use a Workspace Library

Workspace libraries are ideal for commands that are tied to a specific project. Here are the key benefits:

- **Workspace-Aware**: Commands and completions are only available when you are working inside that workspace's directory
  tree.
- **Session-Only**: Workspace items are not saved to your permanent database, keeping your personal library clean.
- **Top Priority**: Workspace-specific commands always appear at the top of your search results.
- **Version Controllable**: You can commit `.intellishell` to your Git repository to share common workspace commands with
    your entire team.
- **Team Onboarding**: New team members can instantly access common build, test, and deployment commands.
- **Discoverability**: Make project-specific scripts and tools easily discoverable—just hit <kbd>Ctrl</kbd>+<kbd>Space</kbd>
    to see what's available.

For your own personal commands that you use everywhere, it is still best to **import them into your permanent library**.
This allows you to benefit from features like variable suggestion history and usage-based ranking, which do not apply to
temporary workspace commands.

## File Format

The `.intellishell` file uses the same simple text format as the `import` and `export` commands. Any commented line (`#`)
before a command is treated as its description. You can check IntelliShell's
[own file](https://github.com/lasantosr/intelli-shell/blob/main/.intellishell) for a real-world example.

Here is a sample `.intellishell` file for a Terraform project:

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

> 💡 **Tip**: For more details on the file format and its syntax, see the [**File Format**](./syncing_and_sharing.md#file-format)
> section of the _Syncing and Sharing_ chapter.

---

With your personal and workspace libraries set up, let's look at how to back up, restore, and share your personal
commands in [**Syncing and Sharing**](./syncing_and_sharing.md).
