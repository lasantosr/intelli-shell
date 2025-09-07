use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};

use crate::widgets::{CommandWidget, VariableCompletionWidget};

/// A widget for `ImportExportItem`, holding both variants
#[derive(Clone)]
pub enum ImportExportItemWidget<'a> {
    Command(CommandWidget<'a>),
    Completion(VariableCompletionWidget<'a>),
}

impl<'a> Widget for ImportExportItemWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        match self {
            ImportExportItemWidget::Command(w) => w.render(area, buf),
            ImportExportItemWidget::Completion(w) => w.render(area, buf),
        }
    }
}
