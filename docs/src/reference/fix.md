# `fix`

The `fix` command provides a **non-interactive** way to correct a failing command using AI. It's the command-line
equivalent of the <kbd>Ctrl</kbd>+<kbd>X</kbd> hotkey, designed for use in scripts or automated workflows.

It acts as an intelligent wrapper that executes a command and, if it fails, uses AI to diagnose the
problem and suggest a solution.

All diagnostic messages are printed to `stderr`, while the corrected command (if any) is printed to `stdout`, allowing
it to be used programmatically.

## How It Works

Tt first executes the command. If the command fails (i.e., exits with a non-zero status code), `fix` captures the output
and sends it, along with some context, to the configured AI for analysis.

The AI's diagnosis is printed to `stderr`, while the clean, corrected command (if one is found) is printed to `stdout`.
This separation allows you to easily pipe the corrected command to another process or variable.

> ⚠️ **Important**
>
> The `fix` command is intended for **non-interactive** shell commands (e.g., `ls`, `git`, `docker`). Executing
> interactive applications like `vim` or `less` through it may lead to unexpected behavior.

## Usage

```sh
intelli-shell fix <COMMAND>
```

## Arguments

- **`<COMMAND>`**: The full command string to execute and potentially fix. This argument is **required**.

## Example

Here is what happens when you run a command with a common typo through `intelli-shell fix`.

**Command:**

```sh
intelli-shell fix "git comit amend"
```

**Output:**

```txt
> git comit amend
git: 'comit' is not a git command. See 'git --help'.

The most similar command is
        commit

────────────────────────────────────────────────────────────────────────────────

🧠 IntelliShell Diagnosis

❌ Git Command Typo
The command "git comit amend" failed because "comit" is a misspelling of the 
"commit" subcommand. Git recognized "comit" as an unrecognized command and 
suggested "commit" as the most similar valid command. This error often occurs 
due to a simple typographical mistake.

✨ Fix
To fix this, correct the spelling of "comit" to "commit". The "--amend" flag 
is commonly used with "git commit" to modify the most recent commit.

Suggested Command 👉
git commit --amend
```

In this example, all the informational text is sent to `stderr`.

Only the final, corrected command is sent to `stdout`, making it safe to use in scripts like
`fixed_cmd=$(intelli-shell fix "git comit amend")`.
