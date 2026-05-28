use crate::scheduler::sm2;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

/// Rating bar showing Again/Hard/Good/Easy with interval previews.
pub struct RatingBar {
    intervals: [i32; 4],
}

impl RatingBar {
    pub fn new(intervals: [i32; 4]) -> Self {
        Self { intervals }
    }
}

impl Widget for RatingBar {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let labels = [
            ("1:Again", Color::Red),
            ("2:Hard", Color::Yellow),
            ("3:Good", Color::Green),
            ("4:Easy", Color::Cyan),
        ];

        let spans: Vec<Span> = labels
            .iter()
            .zip(self.intervals.iter())
            .flat_map(|((label, color), &ivl)| {
                vec![
                    Span::styled(
                        format!(" {label}"),
                        Style::default()
                            .fg(*color)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("({})", sm2::format_interval(ivl)),
                        Style::default().fg(Color::DarkGray),
                    ),
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
