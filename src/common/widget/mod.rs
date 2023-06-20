mod command;
mod label;
mod list;
mod text;

use std::ops::Add;

pub use command::*;
pub use label::*;
pub use list::*;
use ratatui::{
    backend::Backend,
    layout::Rect,
    widgets::{StatefulWidget, Widget},
    Frame,
};
pub use text::*;

use crate::theme::Theme;

// Represents an offset
#[derive(Default, Clone, Copy)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct Offset {
    pub x: u16,
    pub y: u16,
}

impl Offset {
    pub fn new(x: u16, y: u16) -> Self {
        Self { x, y }
    }
}

impl Add<Offset> for Offset {
    type Output = Offset;

    fn add(self, other: Offset) -> Offset {
        Offset {
            x: self.x + other.x,
            y: self.y + other.y,
        }
    }
}

// Represents an area
#[derive(Clone, Copy)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct Area {
    pub width: u16,
    pub height: u16,
}
impl Default for Area {
    fn default() -> Self {
        Self { width: 1, height: 1 }
    }
}

impl Area {
    pub fn default_visible() -> Self {
        Self { width: 25, height: 2 }
    }

    pub fn new(width: u16, height: u16) -> Self {
        Self { width, height }
    }

    pub fn min_width(mut self, width: u16) -> Self {
        self.width = width.max(self.width);
        self
    }
}

/// Custom widget
pub trait CustomWidget<'s> {
    type Inner: Widget;

    /// Retrieves the minimum size needed to render this widget
    fn min_size(&self) -> Area;

    /// Determines if the widget is currently focused
    fn is_focused(&self) -> bool;

    /// Prepares both cursor offset (relative to the area) and widget parts
    fn prepare(&'s self, area: Rect, theme: Theme) -> (Option<Offset>, Self::Inner);

    /// Renders itself in the frame and places the cursor if needed
    fn render_in<B: Backend>(&'s self, frame: &mut Frame<B>, area: Rect, theme: Theme)
    where
        Self: Sized,
    {
        let (offset, widget) = self.prepare(area, theme);
        frame.render_widget(widget, area);

        if self.is_focused() {
            if let Some(offset) = offset {
                // Make the cursor visible and ask tui-rs to put it at the specified coordinates after rendering
                frame.set_cursor(area.x + offset.x, area.y + offset.y);
            }
        }
    }
}

pub trait CustomStatefulWidget<'s> {
    type Inner: StatefulWidget;

    /// Retrieves the minimum size needed to render this widget
    fn min_size(&self) -> Area;

    /// Determines if the widget is currently focused
    fn is_focused(&self) -> bool;

    /// Prepares widget and state parts
    fn prepare(
        &'s mut self,
        area: Rect,
        theme: Theme,
    ) -> (
        Option<Offset>,
        Self::Inner,
        &'s mut <Self::Inner as StatefulWidget>::State,
    );

    /// Renders itself in the frame
    fn render_in<B: Backend>(&'s mut self, frame: &mut Frame<B>, area: Rect, theme: Theme)
    where
        Self: Sized,
    {
        let focused = self.is_focused();
        let (offset, widget, state) = self.prepare(area, theme);
        frame.render_stateful_widget(widget, area, state);

        if focused {
            if let Some(offset) = offset {
                // Make the cursor visible and ask tui-rs to put it at the specified coordinates after rendering
                frame.set_cursor(area.x + offset.x, area.y + offset.y);
            }
        }
    }
}

pub trait IntoWidget<W> {
    /// Converts self into a widget
    fn into_widget(self, theme: Theme) -> W;
}

impl<W, T> IntoWidget<W> for T
where
    T: Into<W>,
{
    fn into_widget(self, _theme: Theme) -> W {
        self.into()
    }
}

pub trait IntoCursorWidget<W> {
    /// Converts self into a widget and its cursor
    fn into_widget_and_cursor(self, theme: Theme) -> (W, Option<(Offset, Area)>);
}

impl<W, T> IntoCursorWidget<W> for T
where
    T: IntoWidget<W>,
{
    fn into_widget_and_cursor(self, theme: Theme) -> (W, Option<(Offset, Area)>) {
        (self.into_widget(theme), None)
    }
}
