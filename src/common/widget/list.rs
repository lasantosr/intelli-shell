use itertools::Itertools;
use tui::{
    layout::Rect,
    style::Style,
    widgets::{Block, Borders, List, ListItem, ListState},
};

use super::{Area, CustomStatefulWidget, IntoWidget};
use crate::theme::Theme;

pub const DEFAULT_HIGHLIGHT_SYMBOL_PREFIX: &str = ">> ";

pub struct CustomStatefulList<T> {
    state: ListState,
    items: Vec<T>,
    inline: bool,
    block_title: Option<&'static str>,

    style: Style,
    highlight_style: Style,
    highlight_symbol: Option<&'static str>,
}

impl<'s, T: 's> CustomStatefulList<T>
where
    &'s T: IntoWidget<ListItem<'s>>,
{
    /// Builds a new list from the given items
    pub fn new(items: Vec<T>) -> Self {
        let mut state = ListState::default();
        if !items.is_empty() {
            state.select(Some(0));
        }
        Self {
            state,
            items,
            inline: true,
            block_title: None,
            style: Style::default(),
            highlight_style: Style::default(),
            highlight_symbol: None,
        }
    }

    pub fn inline(mut self, inline: bool) -> Self {
        self.inline = inline;
        self
    }

    pub fn block_title(mut self, block_title: &'static str) -> Self {
        self.block_title = Some(block_title);
        self
    }

    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    pub fn highlight_symbol(mut self, highlight_symbol: &'static str) -> Self {
        self.highlight_symbol = Some(highlight_symbol);
        self
    }

    pub fn highlight_style(mut self, style: Style) -> Self {
        self.highlight_style = style;
        self
    }

    /// Resets the internal selected state
    pub fn reset_state(&mut self) {
        self.state = ListState::default();
        if !self.items.is_empty() {
            self.state.select(Some(0));
        }
    }

    /// Updates this list inner items
    pub fn update_items(&mut self, items: Vec<T>) {
        self.items = items;

        if self.items.is_empty() {
            self.state.select(None);
        } else if let Some(selected) = self.state.selected() {
            if selected > self.items.len() - 1 {
                self.state.select(Some(self.items.len() - 1));
            }
        } else {
            self.state.select(Some(0));
        }
    }

    /// Returns the number of items on this list
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Selects the next item on the list
    pub fn next(&mut self) {
        if let Some(selected) = self.state.selected() {
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

    /// Selects the previous item on the list
    pub fn previous(&mut self) {
        if let Some(selected) = self.state.selected() {
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

    /// Selects the first item on the list
    pub fn first(&mut self) {
        if !self.items.is_empty() {
            self.state.select(Some(0));
        }
    }

    /// Selects the last item on the list
    pub fn last(&mut self) {
        if !self.items.is_empty() {
            self.state.select(Some(self.items.len() - 1))
        }
    }

    /// Returns a mutable reference to the current selected item
    pub fn current_mut(&mut self) -> Option<&mut T> {
        if let Some(selected) = self.state.selected() {
            self.items.get_mut(selected)
        } else {
            None
        }
    }

    /// Returns a reference to the current selected item
    pub fn current(&self) -> Option<&T> {
        if let Some(selected) = self.state.selected() {
            self.items.get(selected)
        } else {
            None
        }
    }

    /// Deletes the currently selected item and returns it
    pub fn delete_current(&mut self) -> Option<T> {
        let deleted = if let Some(selected) = self.state.selected() {
            Some(self.items.remove(selected))
        } else {
            None
        };

        if self.items.is_empty() {
            self.state.select(None);
        } else if let Some(selected) = self.state.selected() {
            if selected > self.items.len() - 1 {
                self.state.select(Some(self.items.len() - 1));
            }
        } else {
            self.state.select(Some(0));
        }

        deleted
    }
}

impl<'s, T: 's> CustomStatefulWidget<'s> for CustomStatefulList<T>
where
    &'s T: IntoWidget<ListItem<'s>>,
{
    type Inner = List<'s>;

    fn min_size(&self) -> Area {
        let borders = 2 * (!self.inline as u16);
        let height = 1 + borders;
        let width = Area::default_visible().width + borders;
        Area::new(width, height)
    }

    fn prepare(&'s mut self, _area: Rect, theme: Theme) -> (Self::Inner, &mut ListState) {
        // Get the widget of each item
        let widget_items = self
            .items
            .iter()
            .map(|i| IntoWidget::into_widget(i, theme))
            .collect_vec();

        // Generate the list
        let mut list = List::new(widget_items)
            .style(self.style)
            .highlight_style(self.highlight_style);
        if let Some(highlight_symbol) = self.highlight_symbol {
            list = list.highlight_symbol(highlight_symbol);
        }
        if !self.inline {
            let mut block = Block::default().borders(Borders::ALL);
            if let Some(block_title) = self.block_title {
                block = block.title(format!(" {block_title} "));
            }
            list = list.block(block);
        }

        // Return
        (list, &mut self.state)
    }
}
