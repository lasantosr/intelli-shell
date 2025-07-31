use ratatui::{prelude::*, style::Style, widgets::Clear};
use semver::Version;

use crate::config::Theme;

/// A banner widget that displays a message about a new version of the application
#[derive(Clone)]
pub struct NewVersionBanner {
    style: Style,
    new_version: Option<Version>,
}

impl NewVersionBanner {
    /// Creates a new [`NewVersionBanner`]
    pub fn new(theme: &Theme, new_version: Option<Version>) -> Self {
        Self {
            style: theme.accent.into(),
            new_version,
        }
    }

    // Retrieves the inner new version
    pub fn inner(&self) -> &Option<Version> {
        &self.new_version
    }

    /// Renders the new version message popup within the given area.
    ///
    /// The message is displayed as an overlay on the last line of the specified `area`.
    pub fn render_in(&mut self, frame: &mut Frame, area: Rect) {
        // Render the new version message as an overlay, if it exists
        if let Some(ref latest_version) = self.new_version {
            // Define the rectangle for the error message overlay: the last line of the component's total area
            let error_overlay_rect = Rect {
                x: area.x,
                y: area.bottom() - 1,
                width: area.width,
                height: 1,
            };

            let message = format!(
                "🚀 New Version Available: {} → {}",
                env!("CARGO_PKG_VERSION"),
                latest_version
            );
            let text = Line::from(message).centered().style(self.style);

            // Clear the area behind it and render it
            frame.render_widget(Clear, error_overlay_rect);
            frame.render_widget(text, error_overlay_rect);
        }
    }
}
