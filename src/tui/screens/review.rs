use crate::tui::widgets::card_content::CardContent;
use crate::tui::widgets::rating_bar::{HintBar, RatingBar};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewPhase {
    ShowFront,
    ShowBack,
}

pub struct ReviewScreen<'a> {
    pub deck_name: &'a str,
    pub new_remaining: u32,
    pub learn_remaining: u32,
    pub review_remaining: u32,
    pub phase: ReviewPhase,
    pub front_lines: &'a [Line<'a>],
    pub back_lines: &'a [Line<'a>],
    pub scroll: u16,
    pub intervals: [i32; 4],
    pub dry_run: bool,
}

impl Widget for ReviewScreen<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let [top_bar, content_area, bottom_bar] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .areas(area);

        // Top bar: deck name + remaining counts
        render_top_bar(
            top_bar,
            buf,
            self.deck_name,
            self.new_remaining,
            self.learn_remaining,
            self.review_remaining,
            self.dry_run,
        );

        // Content area
        match self.phase {
            ReviewPhase::ShowFront => {
                CardContent::new(self.front_lines, self.scroll).render(content_area, buf);
                HintBar.render(bottom_bar, buf);
            }
            ReviewPhase::ShowBack => {
                // Show front + separator + back
                let mut all_lines: Vec<Line> = self.front_lines.to_vec();
                all_lines.push(Line::default());
                all_lines.push(Line::from(Span::styled(
                    "─".repeat(area.width as usize),
                    Style::default().fg(Color::DarkGray),
                )));
                all_lines.push(Line::default());
                all_lines.extend(self.back_lines.iter().cloned());

                CardContent::new(&all_lines, self.scroll).render(content_area, buf);
                RatingBar::new(self.intervals).render(bottom_bar, buf);
            }
        }
    }
}

fn render_top_bar(
    area: Rect,
    buf: &mut Buffer,
    deck_name: &str,
    new_count: u32,
    learn_count: u32,
    review_count: u32,
    dry_run: bool,
) {
    let mut spans = vec![Span::styled(
        format!(" {deck_name} "),
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    )];
    if dry_run {
        spans.push(Span::styled(
            "[DRY RUN] ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
    }
    spans.extend([
        Span::raw("  "),
        Span::styled(
            format!("{new_count}"),
            Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("+", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{learn_count}"),
            Style::default()
                .fg(Color::Red)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("+", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{review_count}"),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    let line = Line::from(spans);
    buf.set_line(area.x, area.y, &line, area.width);
}

/// "Congratulations" screen when no more cards are due.
pub struct DoneScreen<'a> {
    pub deck_name: &'a str,
    pub reviewed: u32,
}

impl Widget for DoneScreen<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let [_, center, _] = Layout::vertical([
            Constraint::Percentage(40),
            Constraint::Length(4),
            Constraint::Min(0),
        ])
        .areas(area);

        let lines = vec![
            Line::from(Span::styled(
                "Congratulations!",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::default(),
            Line::from(format!(
                "You've reviewed {} card{} in {}.",
                self.reviewed,
                if self.reviewed == 1 { "" } else { "s" },
                self.deck_name
            )),
            Line::from(Span::styled(
                "Press q to return to deck selection.",
                Style::default().fg(Color::DarkGray),
            )),
        ];

        let p = ratatui::widgets::Paragraph::new(lines)
            .alignment(ratatui::layout::Alignment::Center);
        p.render(center, buf);
    }
}
