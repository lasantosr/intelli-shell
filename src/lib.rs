//! _Like IntelliSense, but for shells!_
//!
//! ![intelli-shell demo](https://github.com/lasantosr/intelli-shell/raw/HEAD/docs/src/images/demo.gif)
//!
//! IntelliShell is a command-line tool that acts as a smart bookmark manager.
//! It helps you find, organize, and reuse complex shell commands without ever leaving your terminal.
//!
//! # Features
//!
//! - **Seamless Shell Integration**: Search with `ctrl+space`, bookmark with `ctrl+b` or fix with `ctrl+x`
//! - **Dynamic Variables**: Create command templates with `{{variables}}` and replace them on the fly
//! - **AI-Powered Commands**: Generate, fix, and import commands effortlessly using local or remote LLMs
//! - **Highly Configurable**: Tailor search modes, keybindings, themes, and even search-ranking algorithms
//! - **Workspace-Aware**: Automatically discovers and loads commands from your workspace's directory
//! - **Import / Export**: Share your command library using files, HTTP endpoints, or even Gists
//! - **TLDR Integration**: Fetch and import command examples from [tldr](https://github.com/tldr-pages/tldr) pages
//! - **Flexible Interface**: Choose between a non-intrusive (inline) or an immersive (full-screen) TUI
//!
//! To get started, check out the [repository](https://github.com/lasantosr/intelli-shell) or read the
//! [IntelliShell Book](https://lasantosr.github.io/intelli-shell/) for comprehensive guides and examples.

/// Configuration management for the application.
///
/// This module handles loading, parsing, and providing access to the application's configuration settings.
pub mod config;

/// Tracing and logs management for the application.
///
/// This module is responsible for setting up and configuring the application's logging system using the `tracing`
/// crate.
///
/// It initializes a subscriber that directs logs to a dedicated log file within the application's data directory.
pub mod logging;

/// Centralized error and panic handling for the application.
///
/// This module sets up enhanced error reporting using `color-eyre` and configures custom panic hooks.
///
/// It ensures that errors and panics are gracefully handled, logged to the designated log file, and presented to the
/// user.
pub mod errors;

/// Provides a Terminal User Interface (TUI) management system.
///
/// This module orchestrates the TUI lifecycle, including terminal setup (raw mode, alternate screen or inline display),
/// event handling (keyboard, mouse, paste, resize, focus, periodic ticks, and renders), and rendering the user
/// interface.
///
/// It manages the event loop and provides mechanisms to enter and exit the TUI cleanly, restoring the terminal to its
/// original state.
pub mod tui;

/// Defines and parses the command-line interface (CLI) for the application.
///
/// This module uses the `clap` crate to define the structure of the CLI, including subcommands, arguments, and help
/// messages. It handles parsing command-line arguments provided by the user into a structured format that the
/// application can easily process.
///
/// It is the primary entry point for interacting with the application via the command line.
pub mod cli;

/// Contains the main application logic and orchestration.
///
/// This module defines the `App` struct, which serves as the central coordinator for the application. It holds the
/// application's configuration and manages the overall program flow based on the parsed command-line arguments.
///
/// It dispatches execution to specific processes (interactive or non-interactive) and, for interactive processes,
/// manages the Terminal User Interface (TUI) lifecycle, handles events, and processes user actions.
pub mod app;

/// Defines and implements the distinct operational processes or commands the application can run.
///
/// This module provides traits (`Process`, `InteractiveProcess`) to abstract the concept of an executable task within
/// the application, differentiating between those that run non-interactively and those that require a Terminal User
/// Interface.
///
/// It also defines the standard structure for process output (`ProcessOutput`) and contains the specific
/// implementations of these processes.
pub mod process;

/// Provides the building blocks and interface for interactive Terminal User Interface (TUI) elements.
///
/// This module defines the `Component` trait, which is the fundamental interface for any UI element that can be
/// displayed within the TUI.
///
/// Components are responsible for rendering themselves, processing user input events (keyboard, mouse, paste), managing
/// their internal state, and performing periodic updates.
///
/// This module also serves as a container for specific concrete component implementations used by interactive
/// processes.
pub mod component;

/// Contains custom implementations of [`ratatui`] widgets used by components.
pub mod widgets;

/// Defines the core data structures and models for the application's business domain.
///
/// This module serves as a central collection point and namespace for the fundamental data types that represent the key
/// entities and concepts within the application, such as commands and their associated variables.
pub mod model;

/// Encapsulates the core business logic and operations of the application.
///
/// This module contains the implementation of the application's key functionalities, acting upon the data models
/// defined in the `model` module.
///
/// It orchestrates interactions with the `storage` layer to persist and retrieve data and provides the necessary
/// operations consumed by the `process` and potentially other layers.
pub mod service;

/// Provides the data access layer for the application, abstracting the underlying storage implementation.
///
/// This module is responsible for interacting with the persistent data store (currently SQLite).
///
/// It defines the `SqliteStorage` struct and methods for database initialization, applying migrations, and performing
/// data operations related to the application's models.
pub mod storage;

/// Provides various utility functions and extension traits for common tasks.
///
/// This module contains general-purpose helpers that don't fit neatly into other domain-specific modules. This includes
/// functions for string manipulation, as well as potentially other reusable components or patterns.
pub mod utils;

/// Encapsulates all interactions with Large Language Models (LLMs).
///
/// This module provides the client for the application's AI-powered features, such as generating, fixing, and
/// explaining shell commands. It defines a common interface for interacting with different AI providers.
pub mod ai;
