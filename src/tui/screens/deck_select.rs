use crate::db::models::DeckInfo;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, StatefulWidget, Widget};
use std::collections::HashSet;

pub struct DeckSelectScreen<'a> {
    pub decks: &'a [DeckInfo],
    pub collapsed: &'a HashSet<String>,
}

pub struct DeckSelectState {
    pub list_state: ListState,
    /// Set of deck name prefixes that are collapsed.
    pub collapsed: HashSet<String>,
}

impl DeckSelectState {
    pub fn new(decks: &[DeckInfo]) -> Self {
        // Collapse all decks that have children by default
        let mut collapsed = HashSet::new();
        for deck in decks {
            // A deck is a parent if any other deck starts with "name::"
            let prefix = format!("{}::", deck.name);
            if decks.iter().any(|d| d.name.starts_with(&prefix)) {
                collapsed.insert(deck.name.clone());
            }
        }

        let mut list_state = ListState::default();
        let visible = visible_indices(decks, &collapsed);
        if !visible.is_empty() {
            list_state.select(Some(0));
        }
        Self {
            list_state,
            collapsed,
        }
    }

    pub fn toggle_collapse(&mut self, decks: &[DeckInfo]) {
        let visible = visible_indices(decks, &self.collapsed);
        let Some(sel) = self.list_state.selected() else {
            return;
        };
        let Some(&deck_idx) = visible.get(sel) else {
            return;
        };
        let name = &decks[deck_idx].name;

        // Only toggle if this deck has children
        let prefix = format!("{name}::");
        let has_children = decks.iter().any(|d| d.name.starts_with(&prefix));
        if !has_children {
            return;
        }

        if self.collapsed.contains(name) {
            self.collapsed.remove(name);
        } else {
            self.collapsed.insert(name.clone());
        }
    }

    /// Get the deck index in the full deck list for the currently selected visible row.
    pub fn selected_deck_index(&self, decks: &[DeckInfo]) -> Option<usize> {
        let visible = visible_indices(decks, &self.collapsed);
        let sel = self.list_state.selected()?;
        visible.get(sel).copied()
    }

    pub fn next(&mut self, visible_len: usize) {
        if visible_len == 0 {
            return;
        }
        let i = self
            .list_state
            .selected()
            .map(|i| (i + 1) % visible_len)
            .unwrap_or(0);
        self.list_state.select(Some(i));
    }

    pub fn previous(&mut self, visible_len: usize) {
        if visible_len == 0 {
            return;
        }
        let i = self
            .list_state
            .selected()
            .map(|i| if i == 0 { visible_len - 1 } else { i - 1 })
            .unwrap_or(0);
        self.list_state.select(Some(i));
    }
}

/// Return indices into `decks` for decks that should be visible given collapsed state.
pub fn visible_indices(decks: &[DeckInfo], collapsed: &HashSet<String>) -> Vec<usize> {
    decks
        .iter()
        .enumerate()
        .filter(|(_, d)| {
            // A deck is hidden if any of its ancestor prefixes is collapsed
            // e.g. "A::B::C" is hidden if "A" or "A::B" is collapsed
            let mut parts: Vec<&str> = d.name.split("::").collect();
            parts.pop(); // remove the deck's own name segment
            let mut prefix = String::new();
            for (i, part) in parts.iter().enumerate() {
                if i > 0 {
                    prefix.push_str("::");
                }
                prefix.push_str(part);
                if collapsed.contains(&prefix) {
                    return false;
                }
            }
            true
        })
        .map(|(i, _)| i)
        .collect()
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

        // Build visible deck list
        let visible = visible_indices(self.decks, self.collapsed);
        let items: Vec<ListItem> = visible
            .iter()
            .map(|&idx| {
                let d = &self.decks[idx];
                let name_style = Style::default().fg(Color::White);
                let new_style = Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD);
                let learn_style = Style::default().fg(Color::Red);
                let review_style = Style::default().fg(Color::Green);

                let depth = d.name.matches("::").count();
                let indent = "  ".repeat(depth);
                let display_name = d.name.rsplit("::").next().unwrap_or(&d.name);

                // Check if this deck has children
                let prefix = format!("{}::", d.name);
                let has_children = self.decks.iter().any(|dd| dd.name.starts_with(&prefix));
                let collapse_icon = if has_children {
                    if self.collapsed.contains(&d.name) {
                        "▶ "
                    } else {
                        "▼ "
                    }
                } else {
                    "  "
                };

                let line = Line::from(vec![
                    Span::raw(indent),
                    Span::styled(collapse_icon, Style::default().fg(Color::DarkGray)),
                    Span::styled(format!("{display_name:<38}"), name_style),
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
            .highlight_symbol("▸ ");

        StatefulWidget::render(list, list_area, buf, &mut state.list_state);

        // Footer
        let footer = Line::from(vec![
            Span::styled(" Enter ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled("Study", Style::default().fg(Color::DarkGray)),
            Span::raw("  "),
            Span::styled(" Tab ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled("Expand/Collapse", Style::default().fg(Color::DarkGray)),
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
