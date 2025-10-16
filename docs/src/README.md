# Introduction

_Like IntelliSense, but for shells!_

![intelli-shell demo](images/demo.gif)

Welcome to the official guide for IntelliShell. If you find yourself constantly searching your shell history for that
one-trick `awk` command, forgetting the exact flags for `tar`, or re-typing long commands with only minor changes,
then you've come to the right place.

IntelliShell is a powerful command template and snippet manager that goes far beyond a simple history search to
fundamentally improve your command-line experience. It helps you save, find, generate, and fix your most valuable
commands directly within your terminal, turning your command line from a simple execution tool into a structured,
searchable, and **intelligent library**.

## Why IntelliShell?

IntelliShell is built for users who want to be more efficient and organized in their terminal, with a focus on seamless
integration and user experience.

- **Seamless Shell Integration**: Works directly within your current shell session, making saving and recalling commands
  feel like a native feature rather than an external tool. Trigger it instantly with a simple keybinding
  (`ctrl+space`).

- **Smart & Fast Search**: Find the command you need in milliseconds. The intelligent search and ranking algorithm lets
  you find commands even if you only remember a keyword or two.

- **Powerful Command Templates**: Create reusable command templates using `{{variables}}`. IntelliShell will prompt you
  to fill them in on the fly, making any command adaptable to new scenarios.

- **Dynamic Completions**: Turn static templates into interactive command builders. Power up your templates by defining
  custom scripts that generate live suggestions for your variables, like listing available git branches or docker
  containers.

- **AI Copilot**: Bring the power of AI to your command line. Connect to any local or remote language model to generate
  complex commands and completions from natural language or instantly fix errors in your last command.

- **Fully Customizable**: Tailor every aspect to your specific workflow via a simple configuration file—from keybindings
  and themes to search behavior.

- **Effortless Organization**: Use descriptions and hashtags (`#work`, `#maintenance`) to categorize your commands,
  making them easy to filter and find.

- **Sync & Share**: Export your command library to a file, an HTTP endpoint, or a GitHub Gist to back it up or share it
  with your team.

- **Extensible Knowledge**: Instantly boost your library by fetching and importing command examples from the
  community-driven _tldr_ pages (or virtually any blog post or content with AI enabled).

---

Ready to get started? Let's head over to the [**Installation**](./guide/installation.md) page.
