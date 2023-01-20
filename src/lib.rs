#![forbid(unsafe_code)]

pub mod model;
pub mod storage;
pub mod theme;
pub mod widgets;

mod cfg;
mod common;
#[cfg(feature = "tldr")]
mod tldr;

pub use common::{Widget, WidgetOutput};
