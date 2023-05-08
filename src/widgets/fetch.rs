use anyhow::Result;
use crossterm::event::Event;
use tui::{backend::Backend, layout::Rect, Frame};

use crate::{storage::SqliteStorage, theme::Theme, tldr::scrape_tldr_github, Widget, WidgetOutput};

/// Widget to fetch new commands
///
/// This widget will provide no UI, it will perform the job on `peek`
pub struct FetchWidget<'a> {
    /// Storage
    storage: &'a SqliteStorage,
    /// Category to fetch
    category: Option<String>,
}

impl<'a> FetchWidget<'a> {
    pub fn new(category: Option<String>, storage: &'a SqliteStorage) -> Self {
        Self { category, storage }
    }
}

impl<'a> Widget for FetchWidget<'a> {
    fn min_height(&self) -> usize {
        1
    }

    fn peek(&mut self) -> Result<Option<WidgetOutput>> {
        let mut commands = scrape_tldr_github(self.category.as_deref())?;
        let new = self.storage.insert_commands(&mut commands)?;

        if new == 0 {
            Ok(Some(WidgetOutput::message(
                " -> No new commands to retrieve".to_owned(),
            )))
        } else {
            Ok(Some(WidgetOutput::message(format!(" -> Retrieved {new} new commands"))))
        }
    }

    fn render<B: Backend>(&mut self, _frame: &mut Frame<B>, _area: Rect, _inline: bool, _theme: Theme) {
        unreachable!()
    }

    fn process_raw_event(&mut self, _event: Event) -> Result<Option<WidgetOutput>> {
        unreachable!()
    }
}
