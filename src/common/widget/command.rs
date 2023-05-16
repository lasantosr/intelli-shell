use tui::{
    style::Style,
    text::{Span, Spans},
    widgets::ListItem,
};

use super::IntoWidget;
use crate::{model::Command, theme::Theme};

impl<'a> IntoWidget<ListItem<'a>> for &'a Command {
    fn into_widget(self, theme: Theme) -> ListItem<'a> {
        let mut content = vec![
            Span::raw(&self.cmd),
            Span::styled(" # ", Style::default().fg(theme.description)),
            Span::styled(&self.description, Style::default().fg(theme.description)),
        ];
        if let Some(alias) = &self.alias {
            content.insert(0, Span::styled(format!("[{alias}] "), Style::default().fg(theme.alias)))
        }
        ListItem::new(Spans::from(content))
    }
}
