# Introduction

_Like IntelliSense, but for shells!_

![intelli-shell demo](images/demo.gif)

Welcome to the official guide for IntelliShell. If you find yourself constantly searching your shell history for that
one-trick `awk` command, forgetting the exact flags for `tar` or `ffmpeg`, or re-typing long commands with only minor changes,
then you've come to the right place.

IntelliShell is a smart command-line bookmark manager designed to solve these exact problems. It helps you save, find,
organize, and reuse your most valuable commands directly within your terminal, turning your shell history from a chaotic
log into a structured, searchable library.

## Key Features

IntelliShell is built with a focus on efficiency and seamless integration:

- **Instant Access**: Find and execute commands in seconds with an interactive search UI, triggered by a simple
  keybinding (`ctrl+space`).

- **Dynamic Commands**: Create powerful command templates using `{{variables}}`. IntelliShell will prompt you to fill
  them in on the fly, making your commands reusable for any scenario.

- **Effortless Organization**: Use descriptions and hashtags (`#work`, `#gcp`) to categorize your commands, making them
  easy to filter and find.

- **Workspace-Specific Commands**: Automatically load and prioritize workspace-specific commands from `.intellishell`
  files in your current workspace.

- **Sync & Share**: Export your command library to a file, an HTTP endpoint, or a GitHub Gist to back it up or share it
  with your team.

- **Extensible Knowledge**: Instantly boost your library by fetching and importing command examples from the
  community-driven _tldr_ pages.

## How to Use This Book

This documentation is structured into three main sections to help you find what you need quickly:

- **Quick Start** If you're new to IntelliShell, start here.
  
  The guide contains step-by-step tutorials that will take you from installation and basic usage to mastering advanced
  workflows like shell integration and command syncing.

- **Configuration** This section is your reference for personalizing IntelliShell.
  
  Learn how to change keybindings, customize the theme,
  and even fine-tune the search-ranking algorithms to make the tool truly your own.

- **Command Line Tool** Here you will find a detailed, technical breakdown of every command.
  
  It's the perfect place for quick lookups when you need to know exactly which flags are available or what a specific
  option does.

Ready to get started? Let's head over to the [**Installation**](./guide/installation.md) page.
