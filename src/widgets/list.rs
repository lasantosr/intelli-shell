use std::{borrow::Cow, collections::HashSet};

use ratatui::{
    buffer::Buffer,
    layout::{Rect, Size},
    prelude::StatefulWidget,
    style::{Style, Styled},
    widgets::{Block, Borders, Widget},
};
use tui_widget_list::{ListBuilder, ListState, ListView, ScrollAxis};
use unicode_width::UnicodeWidthStr;

use crate::config::Theme;

/// A trait for types that can be rendered inside a [`CustomList`]
pub trait CustomListItem {
    type Widget<'w>: Widget + 'w
    where
        Self: 'w;

    /// Converts the item into a ratatui widget and its size
    fn as_widget<'a>(
        &'a self,
        theme: &Theme,
        inline: bool,
        is_highlighted: bool,
        is_discarded: bool,
    ) -> (Self::Widget<'a>, Size);
}

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
pub struct CustomList<'a, T: CustomListItem> {
    /// Application theme
    theme: Theme,
    /// Whether the TUI is rendered inline or not
    inline: bool,
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
    /// The indices of the items marked as discarded
    discarded_indices: HashSet<usize>,
    /// An optional symbol string to display in front of the selected item
    highlight_symbol: Option<String>,
    /// Determines how the `highlight_symbol` is rendered
    highlight_symbol_mode: HighlightSymbolMode,
    /// The `Style` to apply to the `highlight_symbol`
    highlight_symbol_style: Style,
    /// The `Style` to apply to the `highlight_symbol` when focused
    highlight_symbol_style_focused: Style,
}

impl<'a, T: CustomListItem> CustomList<'a, T> {
    /// Creates a new [`CustomList`]
    pub fn new(theme: Theme, inline: bool, items: Vec<T>) -> Self {
        let mut state = ListState::default();
        if !items.is_empty() {
            state.select(Some(0));
        }
        Self {
            block: (!inline).then(|| Block::default().borders(Borders::ALL).style(theme.primary)),
            axis: ScrollAxis::Vertical,
            items,
            focus: true,
            state,
            discarded_indices: HashSet::new(),
            highlight_symbol_style: theme.primary.into(),
            highlight_symbol_style_focused: theme.highlight_primary_full().into(),
            highlight_symbol: Some(theme.highlight_symbol.clone()).filter(|s| !s.trim().is_empty()),
            highlight_symbol_mode: HighlightSymbolMode::Last,
            theme,
            inline,
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

    /// Sets the style for the highlight symbol when focused
    pub fn highlight_symbol_style_focused(mut self, highlight_symbol_style_focused: Style) -> Self {
        self.highlight_symbol_style_focused = highlight_symbol_style_focused;
        self
    }

    /// Updates the title of the list
    pub fn set_title(&mut self, new_title: impl Into<Cow<'a, str>>) {
        if let Some(ref mut block) = self.block {
            *block = Block::default()
                .borders(Borders::ALL)
                .style(Styled::style(block))
                .title(new_title.into());
        }
    }

    /// Sets the focus state of the list
    pub fn set_focus(&mut self, focus: bool) {
        self.focus = focus;
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

    /// Returns an iterator over the references to items that have not been discarded
    pub fn non_discarded_items(&self) -> impl Iterator<Item = &T> {
        self.items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| if self.is_discarded(index) { None } else { Some(item) })
    }

    /// Checks if an item at a given index is discarded
    pub fn is_discarded(&self, index: usize) -> bool {
        self.discarded_indices.contains(&index)
    }

    /// Updates the items displayed in this list.
    ///
    /// The discarded state will always be reset.
    pub fn update_items(&mut self, items: Vec<T>, keep_selection: bool) {
        self.items = items;
        self.discarded_indices.clear();

        if keep_selection {
            if self.items.is_empty() {
                self.state.select(None);
            } else if let Some(selected) = self.state.selected {
                if selected > self.items.len() - 1 {
                    self.state.select(Some(self.items.len() - 1));
                }
            } else {
                self.state.select(Some(0));
            }
        } else {
            self.state = ListState::default();
            if !self.items.is_empty() {
                self.state.select(Some(0));
            }
        }
    }

    /// Selects the next item in the list, wrapping around to the beginning if at the end
    pub fn select_next(&mut self) {
        if self.focus
            && let Some(selected) = self.state.selected
        {
            if self.items.is_empty() {
                self.state.select(None);
            } else {
                let i = if selected >= self.items.len() - 1 {
                    0
                } else {
                    selected + 1
                };
                self.state.select(Some(i));
            }
        }
    }

    /// Selects the previous item in the list, wrapping around to the end if at the beginning
    pub fn select_prev(&mut self) {
        if self.focus
            && let Some(selected) = self.state.selected
        {
            if self.items.is_empty() {
                self.state.select(None);
            } else {
                let i = if selected == 0 {
                    self.items.len() - 1
                } else {
                    selected - 1
                };
                self.state.select(Some(i));
            }
        }
    }

    /// Selects the first item in the list
    pub fn select_first(&mut self) {
        if self.focus && !self.items.is_empty() {
            self.state.select(Some(0));
        }
    }

    /// Selects the last item in the list
    pub fn select_last(&mut self) {
        if self.focus && !self.items.is_empty() {
            let i = self.items.len() - 1;
            self.state.select(Some(i));
        }
    }

    /// Selects the first item that matches the given predicate
    pub fn select_matching<F>(&mut self, predicate: F) -> bool
    where
        F: FnMut(&T) -> bool,
    {
        if !self.items.is_empty()
            && let Some(index) = self.items.iter().position(predicate)
        {
            self.state.select(Some(index));
            true
        } else {
            false
        }
    }

    /// Selects the given index
    pub fn select(&mut self, index: usize) {
        if self.focus && index < self.items.len() {
            self.state.select(Some(index));
        }
    }

    /// Returns the index of the currently selected item
    pub fn selected_index(&self) -> Option<usize> {
        self.state.selected
    }

    /// Returns a mutable reference to the currently selected item and its index
    pub fn selected_mut(&mut self) -> Option<&mut T> {
        if let Some(selected) = self.state.selected {
            self.items.get_mut(selected)
        } else {
            None
        }
    }

    /// Returns a reference to the currently selected item and its index
    pub fn selected(&self) -> Option<&T> {
        if let Some(selected) = self.state.selected {
            self.items.get(selected)
        } else {
            None
        }
    }

    /// Returns a reference to the currently selected item and its index
    pub fn selected_with_index(&self) -> Option<(usize, &T)> {
        if let Some(selected) = self.state.selected {
            self.items.get(selected).map(|i| (selected, i))
        } else {
            None
        }
    }

    /// Deletes the currently selected item from the list and returns it (along with its index)
    pub fn delete_selected(&mut self) -> Option<(usize, T)> {
        if self.focus {
            let selected = self.state.selected?;
            let deleted = self.items.remove(selected);

            // Update discarded indices
            self.discarded_indices = self
                .discarded_indices
                .iter()
                .filter_map(|&idx| {
                    if idx < selected {
                        // Indices before the deleted one are unaffected
                        Some(idx)
                    } else if idx > selected {
                        // Indices after the deleted one must be decremented
                        Some(idx - 1)
                    } else {
                        // The deleted index itself is removed from the set
                        None
                    }
                })
                .collect();

            if self.items.is_empty() {
                self.state.select(None);
            } else if selected >= self.items.len() {
                self.state.select(Some(self.items.len() - 1));
            }

            Some((selected, deleted))
        } else {
            None
        }
    }

    /// Toggles the discarded state of the currently selected item
    pub fn toggle_discard_selected(&mut self) {
        if let Some(selected) = self.state.selected
            && !self.discarded_indices.remove(&selected)
        {
            self.discarded_indices.insert(selected);
        }
    }

    /// Toggles the discarded state for all items
    pub fn toggle_discard_all(&mut self) {
        if self.items.is_empty() {
            return;
        }
        // If all items are already discarded
        if self.discarded_indices.len() == self.items.len() {
            // Clear the set
            self.discarded_indices.clear();
        } else {
            // Otherwise, discard all
            self.discarded_indices.extend(0..self.items.len());
        }
    }
}

impl<'a, T: CustomListItem> Widget for &mut CustomList<'a, T> {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        if let Some(ref highlight_symbol) = self.highlight_symbol {
            // Render items with the highlight symbol
            render_list_view(
                ListBuilder::new(|ctx| {
                    let is_highlighted = self.focus && ctx.is_selected;
                    let is_discarded = self.discarded_indices.contains(&ctx.index);
                    // Get the base widget and its height from the item
                    let (item_widget, item_size) =
                        self.items[ctx.index].as_widget(&self.theme, self.inline, is_highlighted, is_discarded);

                    // Wrap the widget with a symbol
                    let item = SymbolAndWidget {
                        content: item_widget,
                        content_height: item_size.height,
                        symbol: if ctx.is_selected { highlight_symbol.as_str() } else { "" },
                        symbol_width: highlight_symbol.width() as u16,
                        symbol_mode: self.highlight_symbol_mode,
                        symbol_style: if self.focus {
                            self.highlight_symbol_style_focused
                        } else {
                            self.highlight_symbol_style
                        },
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
                &mut self.state,
                area,
                buf,
            );
        } else {
            // No highlight symbol, render items directly
            render_list_view(
                ListBuilder::new(|ctx| {
                    let is_highlighted = ctx.is_selected;
                    let is_discarded = self.discarded_indices.contains(&ctx.index);
                    let (item_widget, item_size) =
                        self.items[ctx.index].as_widget(&self.theme, self.inline, is_highlighted, is_discarded);
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
                &mut self.state,
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
