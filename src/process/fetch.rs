use anyhow::Result;
use crossterm::event::Event;
use tui::{backend::Backend, layout::Rect, Frame};

use crate::{storage::SqliteStorage, tldr::scrape_tldr_github, Process, ProcessOutput};

/// Process to fetch new commands
///
/// This process will provide no UI, it will perform the job on `peek`
pub struct FetchProcess<'a> {
    /// Storage
    storage: &'a SqliteStorage,
    /// Category to fetch
    category: Option<String>,
}

impl<'a> FetchProcess<'a> {
    pub fn new(category: Option<String>, storage: &'a SqliteStorage) -> Self {
        Self { category, storage }
    }
}

impl<'a> Process for FetchProcess<'a> {
    fn min_height(&self) -> usize {
        1
    }

    fn peek(&mut self) -> Result<Option<ProcessOutput>> {
        let mut commands = scrape_tldr_github(self.category.as_deref())?;
        let new = self.storage.insert_commands(&mut commands)?;

        if new == 0 {
            Ok(Some(ProcessOutput::message(
                " -> No new commands to retrieve".to_owned(),
            )))
        } else {
            Ok(Some(ProcessOutput::message(format!(
                " -> Retrieved {new} new commands"
            ))))
        }
    }

    fn render<B: Backend>(&mut self, _frame: &mut Frame<B>, _area: Rect) {
        unreachable!()
    }

    fn process_raw_event(&mut self, _event: Event) -> Result<Option<ProcessOutput>> {
        unreachable!()
    }
}
