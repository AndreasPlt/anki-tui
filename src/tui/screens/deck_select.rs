use crate::db::models::DeckInfo;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, StatefulWidget, Widget};

pub struct DeckSelectScreen<'a> {
    pub decks: &'a [DeckInfo],
}

pub struct DeckSelectState {
    pub list_state: ListState,
}

impl DeckSelectState {
    pub fn new(deck_count: usize) -> Self {
        let mut list_state = ListState::default();
        if deck_count > 0 {
            list_state.select(Some(0));
        }
        Self { list_state }
    }

    pub fn selected(&self) -> Option<usize> {
        self.list_state.selected()
    }

    pub fn next(&mut self, len: usize) {
        if len == 0 {
            return;
        }
        let i = self.list_state.selected().map(|i| (i + 1) % len).unwrap_or(0);
        self.list_state.select(Some(i));
    }

    pub fn previous(&mut self, len: usize) {
        if len == 0 {
            return;
        }
        let i = self
            .list_state
            .selected()
            .map(|i| if i == 0 { len - 1 } else { i - 1 })
            .unwrap_or(0);
        self.list_state.select(Some(i));
    }
}

impl StatefulWidget for DeckSelectScreen<'_> {
    type State = DeckSelectState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let [header_area, list_area, footer_area] = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .areas(area);

        // Header
        let header = Line::from(vec![Span::styled(
            " Anki TUI — Select Deck ",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )]);
        let header_block = Block::default().borders(Borders::BOTTOM);
        Widget::render(
            ratatui::widgets::Paragraph::new(header)
                .block(header_block)
                .alignment(ratatui::layout::Alignment::Center),
            header_area,
            buf,
        );

        // Deck list
        let items: Vec<ListItem> = self
            .decks
            .iter()
            .map(|d| {
                let name_style = Style::default().fg(Color::White);
                let new_style = Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD);
                let learn_style = Style::default().fg(Color::Red);
                let review_style = Style::default().fg(Color::Green);

                // Indent based on hierarchy depth
                let depth = d.name.matches("::").count();
                let indent = "  ".repeat(depth);
                let display_name = d.name.rsplit("::").next().unwrap_or(&d.name);

                let line = Line::from(vec![
                    Span::raw(indent),
                    Span::styled(format!("{display_name:<40}"), name_style),
                    Span::styled(format!("{:>4}", d.new_count), new_style),
                    Span::raw(" "),
                    Span::styled(format!("{:>4}", d.learn_count), learn_style),
                    Span::raw(" "),
                    Span::styled(format!("{:>4}", d.review_count), review_style),
                ]);
                ListItem::new(line)
            })
            .collect();

        let list = List::new(items)
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");

        StatefulWidget::render(list, list_area, buf, &mut state.list_state);

        // Footer
        let footer = Line::from(vec![
            Span::styled(" Enter ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled("Study", Style::default().fg(Color::DarkGray)),
            Span::raw("  "),
            Span::styled(" q ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled("Quit", Style::default().fg(Color::DarkGray)),
        ]);
        buf.set_line(
            footer_area.x + 1,
            footer_area.y,
            &footer,
            footer_area.width,
        );
    }
}
