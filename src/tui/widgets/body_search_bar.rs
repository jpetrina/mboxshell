//! In-body search bar shown at the bottom while searching within a message.
//!
//! Mirrors the global [`search_bar`](super::search_bar) but scopes the query to
//! the currently open message body, with a match counter and navigation hints.

use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::i18n;
use crate::tui::app::App;
use crate::tui::theme::current_theme;

/// Render the in-body search prompt with a match counter and navigation hints.
pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    let theme = current_theme();

    let mut spans: Vec<Span<'static>> = vec![Span::styled(" /", theme.search_prompt)];
    spans.push(Span::styled(
        app.body_search_query.clone(),
        theme.message_body,
    ));
    spans.push(Span::styled("_", theme.search_prompt)); // cursor indicator

    if app.body_search_query.is_empty() {
        spans.push(Span::styled(
            format!("  {}", i18n::tui_body_search_hint()),
            theme.help_dim,
        ));
    } else if app.body_search_matches.is_empty() {
        spans.push(Span::styled(
            format!("  {}", i18n::tui_body_search_no_match()),
            theme.help_dim,
        ));
    } else {
        let counter = format!(
            "  ({}/{})  {}",
            app.body_search_index + 1,
            app.body_search_matches.len(),
            i18n::tui_body_search_nav_hint(),
        );
        spans.push(Span::styled(counter, theme.help_dim));
    }

    let bar = Paragraph::new(Line::from(spans)).style(theme.status_bar);
    frame.render_widget(bar, area);
}
