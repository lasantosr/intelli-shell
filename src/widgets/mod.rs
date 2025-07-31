mod command;
mod dynamic;
mod error;
mod list;
mod new_version;
mod tag;
mod textarea;
mod variable;

pub use command::*;
pub use dynamic::*;
pub use error::*;
pub use list::*;
pub use new_version::*;
pub use tag::*;
pub use textarea::*;
pub use variable::*;

/// A trait for types that can be converted into a Ratatui widget for display
pub trait AsWidget {
    /// Sets the highlighted status
    fn set_highlighted(&mut self, is_highlighted: bool) {
        let _ = is_highlighted;
    }

    /// Converts the object into a Ratatui widget and its size
    fn as_widget<'a>(&'a self, is_highlighted: bool) -> (impl ratatui::widgets::Widget + 'a, ratatui::layout::Size);
}
