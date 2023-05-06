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
//!   - Full Text Search in both command and description
//! - Find & replace labels of currently typed command
//! - Non-intrusive (inline) and full-screen interfaces
//! - Fetch command to parse and store [tldr](https://github.com/tldr-pages/tldr) pages (Thanks to them!)
//! - Portability. You can use bookmarked commands in any supported shell, as well as exporting and importing elsewhere.

#![forbid(unsafe_code)]

pub mod model;
pub mod storage;
pub mod theme;
pub mod widgets;

mod cfg;
mod common;
#[cfg(feature = "tldr")]
mod tldr;

pub use common::{ResultExt, Widget, WidgetOutput};
