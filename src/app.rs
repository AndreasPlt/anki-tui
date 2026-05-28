use crate::db::models::{CardRow, DeckInfo, DeckRow};
use crate::db::{mutations, queries};
use crate::error::Result;
use crate::media::kitty;
use crate::proto::deck_config::DeckSchedulingConfig;
use crate::proto::notetype_config;
use crate::proto::template_config;
use crate::scheduler::answer::{self, ReviewTimer};
use crate::scheduler::queue;
use crate::scheduler::sm2::{self, Rating};
use crate::scheduler::timing::{self, SchedTiming};
use crate::template::{html_to_tui, render};
use crate::tui::event::{self, AppEvent};
use crate::tui::screens::deck_select::{visible_indices, DeckSelectScreen, DeckSelectState};
use crate::tui::screens::review::{DoneScreen, ReviewPhase, ReviewScreen};
use crate::tui::terminal::Tui;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::text::Line;
use ratatui::widgets::{StatefulWidget, Widget};
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub struct App {
    conn: Connection,
    media_dir: PathBuf,
    timing: SchedTiming,
    screen: Screen,
    should_quit: bool,
    kitty_supported: bool,
}

enum Screen {
    DeckSelect {
        decks: Vec<DeckInfo>,
        deck_rows: Vec<DeckRow>,
        state: DeckSelectState,
    },
    Review(ReviewState),
    Done {
        deck_name: String,
        reviewed: u32,
    },
}

struct ReviewState {
    deck_name: String,
    deck_ids: Vec<i64>,
    queue: Vec<CardRow>,
    current_idx: usize,
    phase: ReviewPhase,
    scroll: u16,
    front_lines: Vec<Line<'static>>,
    back_lines: Vec<Line<'static>>,
    intervals: [i32; 4],
    conf: DeckSchedulingConfig,
    timer: ReviewTimer,
    reviewed_count: u32,
    new_remaining: u32,
    learn_remaining: u32,
    review_remaining: u32,
    front_images: Vec<html_to_tui::ImageRef>,
    back_images: Vec<html_to_tui::ImageRef>,
}

fn load_card_content(
    conn: &Connection,
    media_dir: &Path,
    timing: &SchedTiming,
    rs: &mut ReviewState,
) -> Result<()> {
    let card = &rs.queue[rs.current_idx];
    let note = queries::load_note(conn, card.nid)?;
    let field_names = queries::load_field_names(conn, note.mid)?;
    let template_row = queries::load_template(conn, note.mid, card.ord)?;
    let notetype = queries::load_notetype(conn, note.mid)?;

    let tmpl_config = template_config::decode_template_config(&template_row.config)?;
    let _nt_config = notetype_config::decode_notetype_config(&notetype.config);

    let field_map = render::build_field_map(&note.flds, &field_names);

    let front_html = render::render_template(&tmpl_config.qfmt, &field_map, None, card.ord);
    let front_rendered = html_to_tui::html_to_lines(&front_html, media_dir);
    rs.front_lines = front_rendered.lines;
    rs.front_images = front_rendered.images;

    let back_html =
        render::render_template(&tmpl_config.afmt, &field_map, Some(&front_html), card.ord);
    let back_rendered = html_to_tui::html_to_lines(&back_html, media_dir);
    rs.back_lines = back_rendered.lines;
    rs.back_images = back_rendered.images;

    // Only compute days_late for review cards (due is a day number)
    let days_late = if card.queue == 2 {
        (timing.days_elapsed - card.due).max(0) as i32
    } else {
        0
    };
    rs.intervals = sm2::preview_intervals_for_card(
        card.queue,
        card.ctype,
        card.ivl,
        card.factor,
        days_late,
        card.left,
        &rs.conf,
    );

    rs.timer = ReviewTimer::start();
    rs.phase = ReviewPhase::ShowFront;
    rs.scroll = 0;

    Ok(())
}

impl App {
    pub fn new(collection_path: &Path, media_dir: PathBuf) -> Result<Self> {
        let conn = crate::db::connection::open_collection(collection_path)?;
        let creation_secs = queries::get_collection_creation_time(&conn)?;
        let offset_mins = queries::get_creation_offset_mins(&conn).unwrap_or(0);
        let timing = timing::sched_timing_today(creation_secs, offset_mins);

        Ok(Self {
            conn,
            media_dir,
            timing,
            screen: Screen::DeckSelect {
                decks: Vec::new(),
                deck_rows: Vec::new(),
                state: DeckSelectState::new(&[]),
            },
            should_quit: false,
            kitty_supported: kitty::is_kitty_supported(),
        })
    }

    pub fn run(&mut self, terminal: &mut Tui) -> Result<()> {
        self.load_deck_list()?;
        let mut needs_redraw = true;

        while !self.should_quit {
            if needs_redraw {
                self.draw(terminal)?;
                needs_redraw = false;
            }
            match event::poll_event(Duration::from_millis(250))? {
                AppEvent::Key(key) => {
                    self.handle_key(key)?;
                    needs_redraw = true;
                }
                AppEvent::Resize(_, _) => {
                    needs_redraw = true;
                }
                AppEvent::None => {}
            }
        }

        Ok(())
    }

    fn draw(&mut self, terminal: &mut Tui) -> Result<()> {
        terminal.draw(|frame| {
            let area = frame.area();
            match &mut self.screen {
                Screen::DeckSelect {
                    decks, state, ..
                } => {
                    let collapsed = state.collapsed.clone();
                    let screen = DeckSelectScreen {
                        decks,
                        collapsed: &collapsed,
                    };
                    StatefulWidget::render(screen, area, frame.buffer_mut(), state);
                }
                Screen::Review(rs) => {
                    let screen = ReviewScreen {
                        deck_name: &rs.deck_name,
                        new_remaining: rs.new_remaining,
                        learn_remaining: rs.learn_remaining,
                        review_remaining: rs.review_remaining,
                        phase: rs.phase,
                        front_lines: &rs.front_lines,
                        back_lines: &rs.back_lines,
                        scroll: rs.scroll,
                        intervals: rs.intervals,
                    };
                    Widget::render(screen, area, frame.buffer_mut());
                }
                Screen::Done {
                    deck_name,
                    reviewed,
                } => {
                    let screen = DoneScreen {
                        deck_name,
                        reviewed: *reviewed,
                    };
                    Widget::render(screen, area, frame.buffer_mut());
                }
            }
        })?;

        if self.kitty_supported {
            self.render_images()?;
        }

        Ok(())
    }

    fn render_images(&self) -> Result<()> {
        if let Screen::Review(rs) = &self.screen {
            let images = match rs.phase {
                ReviewPhase::ShowFront => &rs.front_images,
                ReviewPhase::ShowBack => &rs.back_images,
            };

            for img in images {
                let visible_line = img.line_index as u16;
                if visible_line >= rs.scroll {
                    let row = 1 + visible_line - rs.scroll; // +1 for top bar
                    let _ = kitty::display_image_at(&img.path, row, 0);
                }
            }
        }
        Ok(())
    }

    fn clear_images(&self) {
        if self.kitty_supported {
            let _ = kitty::clear_images();
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        if event::is_quit(&key) {
            match &self.screen {
                Screen::DeckSelect { .. } => {
                    self.should_quit = true;
                }
                Screen::Review(_) | Screen::Done { .. } => {
                    self.clear_images();
                    self.load_deck_list()?;
                }
            }
            return Ok(());
        }

        match &mut self.screen {
            Screen::DeckSelect {
                decks,
                deck_rows,
                state,
            } => {
                let visible_len = visible_indices(decks, &state.collapsed).len();
                match key.code {
                    KeyCode::Down | KeyCode::Char('j') => state.next(visible_len),
                    KeyCode::Up | KeyCode::Char('k') => state.previous(visible_len),
                    KeyCode::Tab | KeyCode::Char('l') | KeyCode::Right => {
                        state.toggle_collapse(decks);
                    }
                    KeyCode::Char('h') | KeyCode::Left => {
                        // Collapse current deck (if expanded) or go to parent
                        if let Some(idx) = state.selected_deck_index(decks) {
                            let name = &decks[idx].name;
                            if !state.collapsed.contains(name) {
                                let prefix = format!("{name}::");
                                if decks.iter().any(|d| d.name.starts_with(&prefix)) {
                                    state.collapsed.insert(name.clone());
                                }
                            }
                        }
                    }
                    KeyCode::Enter => {
                        if let Some(idx) = state.selected_deck_index(decks) {
                            let deck_id = decks[idx].id;
                            let deck_name = decks[idx].name.clone();
                            // Gather this deck + all child deck IDs
                            let names: Vec<String> =
                                deck_rows.iter().map(|d| d.name.replace('\x1f', "::")).collect();
                            let deck_ids =
                                queue::gather_deck_ids(deck_id, &deck_name, deck_rows, &names);
                            let deck_row =
                                deck_rows.iter().find(|d| d.id == deck_id).cloned();
                            if let Some(dr) = deck_row {
                                self.start_review(deck_ids, deck_name, &dr)?;
                            }
                        }
                    }
                    _ => {}
                }
            }
            Screen::Review(rs) => match rs.phase {
                ReviewPhase::ShowFront => match key.code {
                    KeyCode::Char(' ') | KeyCode::Enter => {
                        rs.phase = ReviewPhase::ShowBack;
                        rs.scroll = 0;
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        rs.scroll = rs.scroll.saturating_add(1);
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        rs.scroll = rs.scroll.saturating_sub(1);
                    }
                    _ => {}
                },
                ReviewPhase::ShowBack => match key.code {
                    KeyCode::Char('1') => { self.clear_images(); self.rate_card(Rating::Again)?; }
                    KeyCode::Char('2') => { self.clear_images(); self.rate_card(Rating::Hard)?; }
                    KeyCode::Char('3') | KeyCode::Char(' ') => { self.clear_images(); self.rate_card(Rating::Good)?; }
                    KeyCode::Char('4') => { self.clear_images(); self.rate_card(Rating::Easy)?; }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if let Screen::Review(rs) = &mut self.screen {
                            rs.scroll = rs.scroll.saturating_add(1);
                        }
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        if let Screen::Review(rs) = &mut self.screen {
                            rs.scroll = rs.scroll.saturating_sub(1);
                        }
                    }
                    _ => {}
                },
            },
            Screen::Done { .. } => {
                self.load_deck_list()?;
            }
        }

        Ok(())
    }

    fn load_deck_list(&mut self) -> Result<()> {
        let deck_rows = queries::load_decks(&self.conn)?;
        let decks = queue::build_deck_list(&self.conn, &self.timing, &deck_rows)?;
        let state = DeckSelectState::new(&decks);
        self.screen = Screen::DeckSelect {
            decks,
            deck_rows,
            state,
        };
        Ok(())
    }

    fn start_review(
        &mut self,
        deck_ids: Vec<i64>,
        deck_name: String,
        deck_row: &DeckRow,
    ) -> Result<()> {
        let conf = queue::get_deck_scheduling_config(&self.conn, deck_row)?;
        let cards = queue::load_review_queue(&self.conn, &deck_ids, &self.timing, &conf)?;

        if cards.is_empty() {
            self.screen = Screen::Done {
                deck_name,
                reviewed: 0,
            };
            return Ok(());
        }

        let (new_remaining, learn_remaining, review_remaining) = queries::deck_due_counts(
            &self.conn,
            &deck_ids,
            self.timing.days_elapsed,
            self.timing.now_secs,
        )?;

        let mut rs = ReviewState {
            deck_name,
            deck_ids,
            queue: cards,
            current_idx: 0,
            phase: ReviewPhase::ShowFront,
            scroll: 0,
            front_lines: Vec::new(),
            back_lines: Vec::new(),
            intervals: [0; 4],
            conf,
            timer: ReviewTimer::start(),
            reviewed_count: 0,
            new_remaining,
            learn_remaining,
            review_remaining,
            front_images: Vec::new(),
            back_images: Vec::new(),
        };

        load_card_content(&self.conn, &self.media_dir, &self.timing, &mut rs)?;
        self.screen = Screen::Review(rs);
        Ok(())
    }

    fn rate_card(&mut self, rating: Rating) -> Result<()> {
        let (card, conf, time_ms, deck_ids, deck_name, queue_len, current_idx, reviewed_count) = {
            let Screen::Review(rs) = &self.screen else {
                return Ok(());
            };
            (
                rs.queue[rs.current_idx].clone(),
                rs.conf.clone(),
                rs.timer.elapsed_ms(),
                rs.deck_ids.clone(),
                rs.deck_name.clone(),
                rs.queue.len(),
                rs.current_idx,
                rs.reviewed_count,
            )
        };

        let (updated_card, revlog) =
            answer::answer_card(&card, rating, &conf, &self.timing, time_ms);
        mutations::commit_review(&self.conn, &updated_card, &revlog)?;

        let next_idx = current_idx + 1;
        if next_idx >= queue_len {
            let more_cards =
                queue::load_review_queue(&self.conn, &deck_ids, &self.timing, &conf)?;
            if more_cards.is_empty() {
                self.screen = Screen::Done {
                    deck_name,
                    reviewed: reviewed_count + 1,
                };
                return Ok(());
            }
            let (new_rem, learn_rem, review_rem) = queries::deck_due_counts(
                &self.conn,
                &deck_ids,
                self.timing.days_elapsed,
                self.timing.now_secs,
            )?;
            let mut rs = ReviewState {
                deck_name,
                deck_ids,
                queue: more_cards,
                current_idx: 0,
                phase: ReviewPhase::ShowFront,
                scroll: 0,
                front_lines: Vec::new(),
                back_lines: Vec::new(),
                intervals: [0; 4],
                conf,
                timer: ReviewTimer::start(),
                reviewed_count: reviewed_count + 1,
                new_remaining: new_rem,
                learn_remaining: learn_rem,
                review_remaining: review_rem,
                front_images: Vec::new(),
                back_images: Vec::new(),
            };
            load_card_content(&self.conn, &self.media_dir, &self.timing, &mut rs)?;
            self.screen = Screen::Review(rs);
        } else {
            let (new_rem, learn_rem, review_rem) = queries::deck_due_counts(
                &self.conn,
                &deck_ids,
                self.timing.days_elapsed,
                self.timing.now_secs,
            )?;
            let Screen::Review(rs) = &mut self.screen else {
                return Ok(());
            };
            rs.current_idx = next_idx;
            rs.reviewed_count += 1;
            rs.new_remaining = new_rem;
            rs.learn_remaining = learn_rem;
            rs.review_remaining = review_rem;
            load_card_content(&self.conn, &self.media_dir, &self.timing, rs)?;
        }

        Ok(())
    }
}
