use tui::{
    style::Style,
    text::{Span, Spans},
    widgets::ListItem,
};

use super::IntoWidget;
use crate::{model::Command, theme::Theme};

impl<'a> IntoWidget<ListItem<'a>> for &'a Command {
    fn into_widget(self, theme: Theme) -> ListItem<'a> {
        let content = Spans::from(vec![
            Span::raw(&self.cmd),
            Span::styled(" # ", Style::default().fg(theme.secondary)),
            Span::styled(&self.description, Style::default().fg(theme.secondary)),
        ]);
        ListItem::new(content)
    }
}
