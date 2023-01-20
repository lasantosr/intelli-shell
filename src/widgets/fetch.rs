use anyhow::Result;

use crate::{storage::SqliteStorage, tldr::scrape_tldr_github, Widget, WidgetOutput};

/// Widget to fetch new commands
///
/// This widget will provide no UI, it will perform the job on `peek`
pub struct FetchWidget<'a> {
    /// Storage
    storage: &'a mut SqliteStorage,
    /// Category to fetch
    category: Option<String>,
}

impl<'a> FetchWidget<'a> {
    pub fn new(category: Option<String>, storage: &'a mut SqliteStorage) -> Self {
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
            Ok(Some(WidgetOutput::message("No new commands to retrieve".to_owned())))
        } else {
            Ok(Some(WidgetOutput::message(format!("Retrieved {new} new commands"))))
        }
    }
}
