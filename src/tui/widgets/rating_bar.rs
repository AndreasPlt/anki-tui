use crate::sidecar::ReviewButton;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

/// Rating bar showing Again/Hard/Good/Easy with interval previews.
pub struct RatingBar<'a> {
    buttons: &'a [ReviewButton],
}

impl<'a> RatingBar<'a> {
    pub fn new(buttons: &'a [ReviewButton]) -> Self {
        Self { buttons }
    }
}

impl Widget for RatingBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let colors = [Color::Red, Color::Yellow, Color::Green, Color::Cyan];
        let spans: Vec<Span> = self
            .buttons
            .iter()
            .filter(|button| button.enabled)
            .flat_map(|button| {
                let color = colors
                    .get(button.rating.saturating_sub(1) as usize)
                    .copied()
                    .unwrap_or(Color::White);
                let label = format!("{}:{}", button.rating, button.label);
                let interval = if button.interval.is_empty() {
                    String::new()
                } else {
                    format!("({})", button.interval)
                };
                vec![
                    Span::styled(
                        format!(" {label}"),
                        Style::default().fg(color).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(interval, Style::default().fg(Color::DarkGray)),
                    Span::raw("  "),
                ]
            })
            .collect();

        let line = Line::from(spans);
        let x = area.x + area.width.saturating_sub(line.width() as u16) / 2;
        buf.set_line(x, area.y, &line, area.width);
    }
}

/// Simple hint bar for when showing the front (pre-reveal).
pub struct HintBar;

impl Widget for HintBar {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let line = Line::from(vec![
            Span::styled(
                " Space ",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Show Answer", Style::default().fg(Color::DarkGray)),
            Span::raw("  "),
            Span::styled(
                " q ",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Back", Style::default().fg(Color::DarkGray)),
        ]);
        let x = area.x + area.width.saturating_sub(line.width() as u16) / 2;
        buf.set_line(x, area.y, &line, area.width);
    }
}
