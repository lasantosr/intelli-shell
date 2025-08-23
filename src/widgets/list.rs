use std::borrow::Cow;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    prelude::StatefulWidget,
    style::Style,
    widgets::{Block, Borders, Widget},
};
use tui_widget_list::{ListBuilder, ListState, ListView, ScrollAxis};
use unicode_width::UnicodeWidthStr;

use super::AsWidget;

/// Defines how a highlight symbol is rendered next to a list item
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HighlightSymbolMode {
    /// Appears only on the first line of the selected item
    First,
    /// Repeats on each line of the selected item
    Repeat,
    /// Appears only on the last line of the selected item
    Last,
}

/// A widget that displays a customizable list of items
pub struct CustomList<'a, T: AsWidget> {
    /// An optional `Block` to surround the list
    block: Option<Block<'a>>,
    /// The scroll axis
    axis: ScrollAxis,
    /// The vector of items to be displayed in the list
    items: Vec<T>,
    /// Whether the list is focused or not
    focus: bool,
    /// The state of the list, managing selection and scrolling
    state: ListState,
    /// An optional symbol string to display in front of the selected item
    highlight_symbol: Option<String>,
    /// Determines how the `highlight_symbol` is rendered
    highlight_symbol_mode: HighlightSymbolMode,
    /// The `Style` to apply to the `highlight_symbol`.
    highlight_symbol_style: Style,
}

impl<'a, T: AsWidget> CustomList<'a, T> {
    /// Creates a new [`CustomList`]
    pub fn new(style: impl Into<Style>, inline: bool, mut items: Vec<T>) -> Self {
        let style = style.into();
        let mut state = ListState::default();
        if let Some(first) = items.first_mut() {
            first.set_highlighted(true);
            state.select(Some(0));
        }
        Self {
            block: (!inline).then(|| Block::default().borders(Borders::ALL).style(style)),
            axis: ScrollAxis::Vertical,
            items,
            focus: true,
            state,
            highlight_symbol_style: style,
            highlight_symbol: None,
            highlight_symbol_mode: HighlightSymbolMode::First,
        }
    }

    /// Sets the scroll axis to horizontal
    pub fn horizontal(mut self) -> Self {
        self.axis = ScrollAxis::Horizontal;
        self
    }

    /// Sets the scroll axis to vertical (the default)
    pub fn vertical(mut self) -> Self {
        self.axis = ScrollAxis::Vertical;
        self
    }

    /// Sets the title for the list
    pub fn title(mut self, title: impl Into<Cow<'a, str>>) -> Self {
        self.set_title(title);
        self
    }

    /// Sets the symbol to be displayed before the selected item
    pub fn highlight_symbol(mut self, highlight_symbol: String) -> Self {
        self.highlight_symbol = Some(highlight_symbol).filter(|s| !s.is_empty());
        self
    }

    /// Sets the rendering mode for the highlight symbol
    pub fn highlight_symbol_mode(mut self, highlight_symbol_mode: HighlightSymbolMode) -> Self {
        self.highlight_symbol_mode = highlight_symbol_mode;
        self
    }

    /// Sets the style for the highlight symbol
    pub fn highlight_symbol_style(mut self, highlight_symbol_style: Style) -> Self {
        self.highlight_symbol_style = highlight_symbol_style;
        self
    }

    /// Updates the title of the list
    pub fn set_title(&mut self, new_title: impl Into<Cow<'a, str>>) {
        if let Some(ref mut block) = self.block {
            *block = block.clone().title(new_title.into());
        }
    }

    /// Sets the focus state of the list
    pub fn set_focus(&mut self, focus: bool) {
        if focus != self.focus {
            self.focus = focus;
            if let Some(selected) = self.state.selected
                && let Some(selected) = self.items.get_mut(selected)
            {
                selected.set_highlighted(focus);
            }
        }
    }

    /// Returns whether the text area is currently focused or not
    pub fn is_focused(&self) -> bool {
        self.focus
    }

    /// Returns the number of items in this list
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Returns `true` if the list contains no items
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Returns a reference to the inner items of the list
    pub fn items(&self) -> &Vec<T> {
        &self.items
    }

    /// Returns a mutable reference to the inner items of the list
    pub fn items_mut(&mut self) -> &mut Vec<T> {
        &mut self.items
    }

    /// Updates the items displayed in this list
    pub fn update_items(&mut self, items: Vec<T>) {
        self.items = items;

        if self.items.is_empty() {
            self.state.select(None);
        } else if let Some(selected) = self.state.selected {
            if selected > self.items.len() - 1 {
                self.state.select(Some(self.items.len() - 1));
            }
        } else {
            self.state.select(Some(0));
        }
        if let Some(selected) = self.state.selected
            && let Some(selected) = self.items.get_mut(selected)
        {
            selected.set_highlighted(true);
        }
    }

    /// Resets the internal selected state
    pub fn reset_selection(&mut self) {
        if self.focus {
            if let Some(selected) = self.state.selected
                && let Some(selected) = self.items.get_mut(selected)
            {
                selected.set_highlighted(false);
            }
            self.state = ListState::default();
            if !self.items.is_empty() {
                self.state.select(Some(0));
                if let Some(selected) = self.items.get_mut(0) {
                    selected.set_highlighted(true);
                }
            }
        }
    }

    /// Selects the next item in the list, wrapping around to the beginning if at the end.
    pub fn select_next(&mut self) {
        if self.focus
            && let Some(selected) = self.state.selected
        {
            if let Some(selected) = self.items.get_mut(selected) {
                selected.set_highlighted(false);
            }
            if self.items.is_empty() {
                self.state.select(None);
            } else {
                let i = if selected >= self.items.len() - 1 {
                    0
                } else {
                    selected + 1
                };
                self.state.select(Some(i));
                if let Some(selected) = self.items.get_mut(i) {
                    selected.set_highlighted(true);
                }
            }
        }
    }

    /// Selects the previous item in the list, wrapping around to the end if at the beginning.
    pub fn select_prev(&mut self) {
        if self.focus
            && let Some(selected) = self.state.selected
        {
            if let Some(selected) = self.items.get_mut(selected) {
                selected.set_highlighted(false);
            }
            if self.items.is_empty() {
                self.state.select(None);
            } else {
                let i = if selected == 0 {
                    self.items.len() - 1
                } else {
                    selected - 1
                };
                self.state.select(Some(i));
                if let Some(selected) = self.items.get_mut(i) {
                    selected.set_highlighted(true);
                }
            }
        }
    }

    /// Selects the first item in the list
    pub fn select_first(&mut self) {
        if self.focus && !self.items.is_empty() {
            if let Some(selected) = self.state.selected
                && let Some(selected) = self.items.get_mut(selected)
            {
                selected.set_highlighted(false);
            }
            self.state.select(Some(0));
            if let Some(selected) = self.items.get_mut(0) {
                selected.set_highlighted(true);
            }
        }
    }

    /// Selects the last item in the list
    pub fn select_last(&mut self) {
        if self.focus && !self.items.is_empty() {
            if let Some(selected) = self.state.selected
                && let Some(selected) = self.items.get_mut(selected)
            {
                selected.set_highlighted(false);
            }
            let i = self.items.len() - 1;
            self.state.select(Some(i));
            if let Some(selected) = self.items.get_mut(i) {
                selected.set_highlighted(true);
            }
        }
    }

    /// Selects the given index
    pub fn select(&mut self, index: usize) {
        if self.focus && index < self.items.len() {
            if let Some(selected) = self.state.selected
                && let Some(selected) = self.items.get_mut(selected)
            {
                selected.set_highlighted(false);
            }
            self.state.select(Some(index));
            if let Some(selected) = self.items.get_mut(index) {
                selected.set_highlighted(true);
            }
        }
    }

    /// Returns the index of the currently selected item
    pub fn selected_index(&self) -> Option<usize> {
        self.state.selected
    }

    /// Returns a mutable reference to the currently selected item
    pub fn selected_mut(&mut self) -> Option<&mut T> {
        if self.focus {
            if let Some(selected) = self.state.selected {
                self.items.get_mut(selected)
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Returns a reference to the currently selected item
    pub fn selected(&self) -> Option<&T> {
        if self.focus {
            if let Some(selected) = self.state.selected {
                self.items.get(selected)
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Returns a reference to the currently selected item and its index
    pub fn selected_with_index(&self) -> Option<(usize, &T)> {
        if self.focus {
            if let Some(selected) = self.state.selected {
                self.items.get(selected).map(|i| (selected, i))
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Deletes the currently selected item from the list and returns it
    pub fn delete_selected(&mut self) -> Option<T> {
        if self.focus {
            let selected = self.state.selected?;
            let mut deleted = self.items.remove(selected);
            deleted.set_highlighted(false);

            if self.items.is_empty() {
                self.state.select(None);
            } else if selected >= self.items.len() {
                self.state.select(Some(self.items.len() - 1));
            }
            if let Some(selected) = self.state.selected
                && let Some(selected) = self.items.get_mut(selected)
            {
                selected.set_highlighted(true);
            }

            Some(deleted)
        } else {
            None
        }
    }
}

impl<'a, T: AsWidget> Widget for &mut CustomList<'a, T> {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let mut default_state = ListState::default();
        let state = if self.focus {
            &mut self.state
        } else {
            &mut default_state
        };
        if let Some(ref highlight_symbol) = self.highlight_symbol {
            // Render items with the highlight symbol
            render_list_view(
                ListBuilder::new(|ctx| {
                    // Get the base widget and its height from the item
                    let (item_widget, item_size) = self.items[ctx.index].as_widget(ctx.is_selected);

                    // Wrap the widget with a symbol
                    let item = SymbolAndWidget {
                        content: item_widget,
                        content_height: item_size.height,
                        symbol: if ctx.is_selected { highlight_symbol.as_str() } else { "" },
                        symbol_width: highlight_symbol.width() as u16,
                        symbol_mode: self.highlight_symbol_mode,
                        symbol_style: self.highlight_symbol_style,
                    };

                    let main_axis_size = match ctx.scroll_axis {
                        ScrollAxis::Vertical => item_size.height,
                        ScrollAxis::Horizontal => item_size.width + 1,
                    };

                    (item, main_axis_size)
                }),
                self.axis,
                self.block.is_none(),
                self.items.len(),
                self.block.clone(),
                state,
                area,
                buf,
            );
        } else {
            // No highlight symbol, render items directly
            render_list_view(
                ListBuilder::new(|ctx| {
                    let (item_widget, item_size) = self.items[ctx.index].as_widget(ctx.is_selected);
                    let main_axis_size = match ctx.scroll_axis {
                        ScrollAxis::Vertical => item_size.height,
                        ScrollAxis::Horizontal => item_size.width + 1,
                    };
                    (item_widget, main_axis_size)
                }),
                self.axis,
                self.block.is_none(),
                self.items.len(),
                self.block.clone(),
                state,
                area,
                buf,
            );
        }
    }
}

/// Internal helper function to render a list view using a generic builder
#[allow(clippy::too_many_arguments)]
fn render_list_view<'a, W: Widget>(
    builder: ListBuilder<'a, W>,
    axis: ScrollAxis,
    inline: bool,
    item_count: usize,
    block: Option<Block<'a>>,
    state: &mut ListState,
    area: Rect,
    buf: &mut Buffer,
) {
    let mut view = ListView::new(builder, item_count)
        .scroll_axis(axis)
        .infinite_scrolling(false)
        .scroll_padding(1 + (!inline as u16));
    if let Some(block) = block {
        view = view.block(block);
    }
    view.render(area, buf, state)
}

/// Internal helper widget to render an item prefixed with a highlight symbol
struct SymbolAndWidget<'a, W: Widget> {
    /// The widget to be rendered as the main content next to the symbol
    content: W,
    /// Height of the `content` widget in rows
    content_height: u16,
    /// The actual symbol string to render (e.g., " > ", " ✔ ").
    symbol: &'a str,
    /// The pre-calculated width of the `symbol` string
    symbol_width: u16,
    /// Specifies how the `symbol` should be rendered relative to the `content`
    symbol_mode: HighlightSymbolMode,
    /// The `Style` to be applied to the `symbol` when rendering.
    symbol_style: Style,
}

impl<'a, W: Widget> Widget for SymbolAndWidget<'a, W> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut content_area = area;
        let mut symbol_area = Rect::default();

        // Calculate the area for the symbol string
        if self.symbol_width > 0 && area.width > 0 {
            symbol_area = Rect {
                x: area.x,
                y: area.y,
                width: self.symbol_width.min(area.width),
                height: area.height,
            };

            // Adjust content area to be to the right of the symbol area
            content_area.x = area.x.saturating_add(symbol_area.width);
            content_area.width = area.width.saturating_sub(symbol_area.width);
        }

        // Render the actual item content widget
        if content_area.width > 0 && content_area.height > 0 {
            self.content.render(content_area, buf);
        }

        // Render the highlight symbol
        if !self.symbol.is_empty() && symbol_area.width > 0 && symbol_area.height > 0 {
            // Fill the entire symbol_area with the background style
            if let Some(bg_color) = self.symbol_style.bg {
                for y_coord in symbol_area.top()..symbol_area.bottom() {
                    for x_coord in symbol_area.left()..symbol_area.right() {
                        if let Some(cell) = buf.cell_mut((x_coord, y_coord)) {
                            cell.set_bg(bg_color);
                        }
                    }
                }
            }
            // Render the symbol
            match self.symbol_mode {
                HighlightSymbolMode::First => {
                    // Render on the first line of the symbol's allocated area
                    buf.set_stringn(
                        symbol_area.x,
                        symbol_area.y,
                        self.symbol,
                        symbol_area.width as usize,
                        self.symbol_style,
                    );
                }
                HighlightSymbolMode::Repeat => {
                    // Repeat for each line of the content, up to content_height or available symbol area height
                    for i in 0..self.content_height {
                        let y_pos = symbol_area.y + i;
                        // Ensure we are within the bounds of the symbol_area
                        if y_pos < symbol_area.bottom() && i < symbol_area.height {
                            buf.set_stringn(
                                symbol_area.x,
                                y_pos,
                                self.symbol,
                                symbol_area.width as usize,
                                self.symbol_style,
                            );
                        } else {
                            // Stop if we go beyond the symbol area's height
                            break;
                        }
                    }
                }
                HighlightSymbolMode::Last => {
                    // Render on the last line occupied by the content, if space permits
                    if self.content_height > 0 {
                        let y_pos = symbol_area.y + self.content_height - 1;
                        // Ensure the calculated y_pos is within the symbol_area
                        if y_pos < symbol_area.bottom() && (self.content_height - 1) < symbol_area.height {
                            buf.set_stringn(
                                symbol_area.x,
                                y_pos,
                                self.symbol,
                                symbol_area.width as usize,
                                self.symbol_style,
                            );
                        }
                    }
                }
            }
        }
    }
}
