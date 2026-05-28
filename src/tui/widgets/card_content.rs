use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Widget, Wrap};

/// Scrollable card content widget.
pub struct CardContent<'a> {
    lines: &'a [Line<'a>],
    scroll: u16,
}

impl<'a> CardContent<'a> {
    pub fn new(lines: &'a [Line<'a>], scroll: u16) -> Self {
        Self { lines, scroll }
    }
}

impl Widget for CardContent<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let paragraph = Paragraph::new(self.lines.to_vec())
            .wrap(Wrap { trim: false })
            .scroll((self.scroll, 0));
        paragraph.render(area, buf);
    }
}

/// Calculate the maximum scroll position.
pub fn max_scroll(total_lines: usize, visible_height: u16) -> u16 {
    total_lines.saturating_sub(visible_height as usize) as u16
}
