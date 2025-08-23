use ratatui::{prelude::*, style::Style, widgets::Clear};
use semver::Version;

use crate::config::Theme;

/// A banner widget that displays a message about a new version of the application
#[derive(Clone)]
pub struct NewVersionBanner {
    style: Style,
    new_version: Version,
}

impl NewVersionBanner {
    /// Creates a new [`NewVersionBanner`]
    pub fn new(theme: &Theme, new_version: Version) -> Self {
        Self {
            style: theme.accent.into(),
            new_version,
        }
    }

    /// Renders the new version message popup within the given area.
    ///
    /// The message is displayed as an overlay on the last line of the specified `area`.
    pub fn render_in(&mut self, frame: &mut Frame, area: Rect) {
        // Render the new version message as an overlay
        let error_overlay_rect = Rect {
            x: area.x,
            y: area.bottom() - 1,
            width: area.width,
            height: 1,
        };

        let message = format!(
            "🚀 New Version Available: {} → {}",
            env!("CARGO_PKG_VERSION"),
            self.new_version
        );
        let text = Line::from(message).centered().style(self.style);

        // Clear the area behind it and render it
        frame.render_widget(Clear, error_overlay_rect);
        frame.render_widget(text, error_overlay_rect);
    }
}
