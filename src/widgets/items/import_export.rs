use ratatui::layout::Size;

use crate::{
    config::Theme,
    model::ImportExportItem,
    widgets::{
        CustomListItem, ImportExportItemWidget,
        items::{PlainStyleCommand, PlainStyleVariableCompletion},
    },
};

/// Wrapper around `ImportExportItem` to be rendered on a plain style
#[derive(Clone)]
pub enum PlainStyleImportExportItem {
    Command(PlainStyleCommand),
    Completion(PlainStyleVariableCompletion),
}

impl CustomListItem for PlainStyleImportExportItem {
    type Widget<'w> = ImportExportItemWidget<'w>;

    fn as_widget<'a>(
        &'a self,
        theme: &Theme,
        inline: bool,
        is_highlighted: bool,
        is_discarded: bool,
    ) -> (Self::Widget<'a>, Size) {
        match self {
            PlainStyleImportExportItem::Command(command) => {
                let (widget, size) = command.as_widget(theme, inline, is_highlighted, is_discarded);
                (ImportExportItemWidget::Command(widget), size)
            }
            PlainStyleImportExportItem::Completion(completion) => {
                let (widget, size) = completion.as_widget(theme, inline, is_highlighted, is_discarded);
                (ImportExportItemWidget::Completion(widget), size)
            }
        }
    }
}

impl From<ImportExportItem> for PlainStyleImportExportItem {
    fn from(value: ImportExportItem) -> Self {
        match value {
            ImportExportItem::Command(command) => Self::Command(command.into()),
            ImportExportItem::Completion(completion) => Self::Completion(completion.into()),
        }
    }
}
impl From<PlainStyleImportExportItem> for ImportExportItem {
    fn from(value: PlainStyleImportExportItem) -> Self {
        match value {
            PlainStyleImportExportItem::Command(command) => Self::Command(command.into()),
            PlainStyleImportExportItem::Completion(completion) => Self::Completion(completion.into()),
        }
    }
}
