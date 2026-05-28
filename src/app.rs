use crate::error::Result;
use crate::media::kitty;
use crate::sidecar::{DeckInfo, Rating, ReviewButton, ReviewCard, ReviewSnapshot, SidecarClient};
use crate::template::html_to_tui;
use crate::tui::event::{self, AppEvent};
use crate::tui::screens::deck_select::{DeckSelectScreen, DeckSelectState, visible_indices};
use crate::tui::screens::review::{DoneScreen, ReviewPhase, ReviewScreen};
use crate::tui::terminal::Tui;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::text::Line;
use ratatui::widgets::{Clear, StatefulWidget, Widget};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewMode {
    Live,
    DryRun,
}

pub struct App {
    sidecar: SidecarClient,
    media_dir: PathBuf,
    screen: Screen,
    should_quit: bool,
    kitty_supported: bool,
    review_mode: ReviewMode,
    audio_player: Option<crate::media::audio::AudioPlayer>,
}

enum Screen {
    DeckSelect {
        decks: Vec<DeckInfo>,
        state: DeckSelectState,
    },
    Review(Box<ReviewState>),
    Done {
        deck_name: String,
        reviewed: u32,
    },
}

struct ReviewState {
    deck_id: i64,
    deck_name: String,
    card: ReviewCard,
    phase: ReviewPhase,
    scroll: u16,
    front_lines: Vec<Line<'static>>,
    back_lines: Vec<Line<'static>>,
    buttons: Vec<ReviewButton>,
    reviewed_count: u32,
    new_remaining: u32,
    learn_remaining: u32,
    review_remaining: u32,
    front_images: Vec<html_to_tui::ImageRef>,
    back_images: Vec<html_to_tui::ImageRef>,
    front_audio: Vec<html_to_tui::AudioRef>,
    back_audio: Vec<html_to_tui::AudioRef>,
}

fn state_from_snapshot(
    snapshot: ReviewSnapshot,
    media_dir: &Path,
    reviewed_count: u32,
) -> Option<ReviewState> {
    let card = snapshot.card?;
    let front_rendered = html_to_tui::html_to_lines(&card.question_html, media_dir);
    let back_rendered = html_to_tui::html_to_lines(&card.answer_html, media_dir);
    let mut front_audio = front_rendered.audio;
    extend_audio_refs(&mut front_audio, &card.front_audio, media_dir);
    let mut back_audio = back_rendered.audio;
    extend_audio_refs(&mut back_audio, &card.back_audio, media_dir);
    Some(ReviewState {
        deck_id: snapshot.deck_id,
        deck_name: snapshot.deck_name,
        buttons: card.buttons.clone(),
        card,
        phase: ReviewPhase::ShowFront,
        scroll: 0,
        front_lines: front_rendered.lines,
        back_lines: back_rendered.lines,
        reviewed_count,
        new_remaining: snapshot.counts.new,
        learn_remaining: snapshot.counts.learn,
        review_remaining: snapshot.counts.review,
        front_images: front_rendered.images,
        back_images: back_rendered.images,
        front_audio,
        back_audio,
    })
}

fn extend_audio_refs(
    audio: &mut Vec<html_to_tui::AudioRef>,
    filenames: &[String],
    media_dir: &Path,
) {
    for filename in filenames {
        let path = media_dir.join(filename);
        if !audio.iter().any(|a| a.path == path) {
            audio.push(html_to_tui::AudioRef { path });
        }
    }
}

impl App {
    pub fn new(
        collection_path: &Path,
        media_dir: PathBuf,
        review_mode: ReviewMode,
    ) -> Result<Self> {
        let sidecar = SidecarClient::start(collection_path, &media_dir)?;

        Ok(Self {
            sidecar,
            media_dir,
            screen: Screen::DeckSelect {
                decks: Vec::new(),
                state: DeckSelectState::new(&[]),
            },
            should_quit: false,
            kitty_supported: kitty::is_kitty_supported(),
            review_mode,
            audio_player: crate::media::audio::AudioPlayer::new(),
        })
    }

    pub fn run(&mut self, terminal: &mut Tui) -> Result<()> {
        self.load_deck_list()?;
        let mut needs_redraw = true;
        let mut force_clear = true;

        while !self.should_quit {
            if needs_redraw {
                if force_clear {
                    self.clear_images();
                    terminal.clear()?;
                    force_clear = false;
                }
                self.draw(terminal)?;
                needs_redraw = false;
            }
            match event::poll_event(Duration::from_millis(250))? {
                AppEvent::Key(key) => {
                    let was_review = matches!(self.screen, Screen::Review(_));
                    self.handle_key(key)?;
                    let is_review = matches!(self.screen, Screen::Review(_));
                    if was_review || is_review {
                        force_clear = true;
                    }
                    needs_redraw = true;
                }
                AppEvent::Resize(_, _) => {
                    needs_redraw = true;
                    force_clear = true;
                }
                AppEvent::None => {}
            }
        }

        Ok(())
    }

    fn draw(&mut self, terminal: &mut Tui) -> Result<()> {
        terminal.draw(|frame| {
            let area = frame.area();
            Widget::render(Clear, area, frame.buffer_mut());
            match &mut self.screen {
                Screen::DeckSelect { decks, state } => {
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
                        buttons: &rs.buttons,
                        dry_run: self.review_mode == ReviewMode::DryRun,
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
                    let row = 1 + visible_line - rs.scroll;
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

    fn play_current_audio(&self) {
        let Some(player) = &self.audio_player else {
            return;
        };
        let Screen::Review(rs) = &self.screen else {
            return;
        };
        let audio = match rs.phase {
            ReviewPhase::ShowFront => &rs.front_audio,
            ReviewPhase::ShowBack => &rs.back_audio,
        };
        let paths: Vec<&std::path::Path> = audio.iter().map(|a| a.path.as_path()).collect();
        if !paths.is_empty() {
            player.play(&paths);
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
            Screen::DeckSelect { decks, state } => {
                let visible_len = visible_indices(decks, &state.collapsed).len();
                match key.code {
                    KeyCode::Down | KeyCode::Char('j') => state.next(visible_len),
                    KeyCode::Up | KeyCode::Char('k') => state.previous(visible_len),
                    KeyCode::Tab | KeyCode::Char('l') | KeyCode::Right => {
                        state.toggle_collapse(decks);
                    }
                    KeyCode::Char('h') | KeyCode::Left => {
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
                            self.start_review(deck_id)?;
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
                        self.play_current_audio();
                    }
                    KeyCode::Char('r') => {
                        self.play_current_audio();
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
                    KeyCode::Char('1') => {
                        self.clear_images();
                        self.rate_card(Rating::Again)?;
                    }
                    KeyCode::Char('2') => {
                        self.clear_images();
                        self.rate_card(Rating::Hard)?;
                    }
                    KeyCode::Char('3') | KeyCode::Char(' ') => {
                        self.clear_images();
                        self.rate_card(Rating::Good)?;
                    }
                    KeyCode::Char('4') => {
                        self.clear_images();
                        self.rate_card(Rating::Easy)?;
                    }
                    KeyCode::Char('r') => {
                        self.play_current_audio();
                    }
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
        let decks = self.sidecar.list_decks()?;
        let state = DeckSelectState::new(&decks);
        self.screen = Screen::DeckSelect { decks, state };
        Ok(())
    }

    fn start_review(&mut self, deck_id: i64) -> Result<()> {
        let snapshot = self
            .sidecar
            .start_review(deck_id, self.review_mode == ReviewMode::DryRun)?;
        let deck_name = snapshot.deck_name.clone();
        if let Some(rs) = state_from_snapshot(snapshot, &self.media_dir, 0) {
            self.screen = Screen::Review(Box::new(rs));
            self.play_current_audio();
        } else {
            self.screen = Screen::Done {
                deck_name,
                reviewed: 0,
            };
        }
        Ok(())
    }

    fn rate_card(&mut self, rating: Rating) -> Result<()> {
        let (card_id, deck_id, deck_name, reviewed_count) = {
            let Screen::Review(rs) = &self.screen else {
                return Ok(());
            };
            (
                rs.card.id,
                rs.deck_id,
                rs.deck_name.clone(),
                rs.reviewed_count,
            )
        };

        let snapshot = self.sidecar.answer_card(card_id, rating)?;
        let reviewed = reviewed_count + 1;
        if let Some(rs) = state_from_snapshot(snapshot, &self.media_dir, reviewed) {
            self.screen = Screen::Review(Box::new(rs));
            self.play_current_audio();
        } else {
            self.screen = Screen::Done {
                deck_name: if deck_name.is_empty() {
                    deck_id.to_string()
                } else {
                    deck_name
                },
                reviewed,
            };
        }

        Ok(())
    }
}
