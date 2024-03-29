//! Like IntelliSense, but for shells!
//!
//! ![intelli-shell demo](https://github.com/lasantosr/intelli-shell/raw/HEAD/assets/intellishell.gif)
//!
//! IntelliShell acts like a bookmark store for commands, so you don't have to keep your history clean in order to be
//! able to find something useful with `ctrl + R`.
//!
//! # Features
//!
//! - Standalone binaries
//! - Autocomplete currently typed command
//!   - Full Text Search in both command and description with hashtag support on descriptions
//! - Find & replace labels of currently typed command
//! - Edit bookmarked commands and provide aliases
//! - Non-intrusive (inline) and full-screen interfaces
//! - Fetch command to parse and store [tldr](https://github.com/tldr-pages/tldr) pages (Thanks to them!)
//! - Portability. You can use bookmarked commands in any supported shell, as well as exporting and importing elsewhere.

#![forbid(unsafe_code)]

pub mod debug;
pub mod model;
pub mod process;
pub mod storage;
pub mod theme;

mod cfg;
mod common;
#[cfg(feature = "tldr")]
mod tldr;

pub use common::{remove_newlines, ExecutionContext, Process, ProcessOutput};
