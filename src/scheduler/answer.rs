use crate::db::models::{CardRow, RevlogEntry};
use crate::proto::deck_config::DeckSchedulingConfig;
use crate::scheduler::sm2::{self, Rating};
use crate::scheduler::timing::SchedTiming;
use std::time::Instant;

/// Tracks timing for the current card review.
pub struct ReviewTimer {
    started: Instant,
}

impl ReviewTimer {
    pub fn start() -> Self {
        Self {
            started: Instant::now(),
        }
    }

    pub fn elapsed_ms(&self) -> i32 {
        self.started.elapsed().as_millis().min(i32::MAX as u128) as i32
    }
}

/// Apply a rating to a card and produce the updated card + revlog entry.
pub fn answer_card(
    card: &CardRow,
    rating: Rating,
    conf: &DeckSchedulingConfig,
    timing: &SchedTiming,
    time_ms: i32,
) -> (CardRow, RevlogEntry) {
    let mut new = card.clone();
    let now_ms = timing.now_secs * 1000;

    new.mod_ = timing.now_secs;
    new.usn = -1; // unsynced
    new.reps += 1;

    let (review_type, last_ivl) = match card.queue {
        // New card
        0 => {
            answer_new(&mut new, rating, conf, timing);
            (0, 0)
        }
        // Learning / day-learning
        1 | 3 => {
            let last = if card.ivl < 0 { card.ivl } else { -card.ivl * 86400 };
            answer_learning(&mut new, rating, conf, timing, false);
            (0, last)
        }
        // Review
        2 => {
            let last = card.ivl;
            answer_review(&mut new, rating, conf, timing);
            (1, last)
        }
        _ => (0, 0),
    };

    // Determine review_type for revlog: 0=learn, 1=review, 2=relearn, 3=filtered
    let revlog_type = if card.ctype == 2 && rating == Rating::Again {
        2 // relearn
    } else {
        review_type
    };

    let revlog = RevlogEntry {
        id: now_ms + rand::random::<u16>() as i64 % 1000,
        cid: card.id,
        usn: -1,
        ease: rating as i32,
        ivl: new.ivl,
        last_ivl: last_ivl,
        factor: new.factor,
        time: time_ms,
        review_type: revlog_type,
    };

    (new, revlog)
}

fn answer_new(
    card: &mut CardRow,
    rating: Rating,
    conf: &DeckSchedulingConfig,
    timing: &SchedTiming,
) {
    let steps = &conf.learn_steps_mins;
    let initial_factor = (conf.initial_ease * 1000.0) as i32;

    match rating {
        Rating::Again => {
            // Enter learning at step 0
            card.ctype = 1;
            card.queue = 1;
            card.left = encode_left(steps.len(), steps.len());
            card.due = timing.now_secs + sm2::learning_step_secs(steps, 0);
            card.ivl = 0;
            card.factor = initial_factor;
        }
        Rating::Hard => {
            // Stay at step 0 (repeat)
            card.ctype = 1;
            card.queue = 1;
            card.left = encode_left(steps.len(), steps.len());
            let step0 = sm2::learning_step_secs(steps, 0);
            let step1 = steps.get(1).map(|&m| (m * 60.0) as i64).unwrap_or(step0);
            card.due = timing.now_secs + (step0 + step1) / 2;
            card.ivl = 0;
            card.factor = initial_factor;
        }
        Rating::Good => {
            if steps.len() <= 1 {
                // Graduate immediately
                graduate(card, conf.graduating_interval_good as i32, initial_factor, timing);
            } else {
                // Go to step 1
                card.ctype = 1;
                card.queue = 1;
                card.left = encode_left(steps.len() - 1, steps.len() - 1);
                card.due = timing.now_secs + sm2::learning_step_secs(steps, 1);
                card.ivl = 0;
                card.factor = initial_factor;
            }
        }
        Rating::Easy => {
            graduate(card, conf.graduating_interval_easy as i32, initial_factor, timing);
        }
    }
}

fn answer_learning(
    card: &mut CardRow,
    rating: Rating,
    conf: &DeckSchedulingConfig,
    timing: &SchedTiming,
    is_relearn: bool,
) {
    let steps = if is_relearn {
        &conf.relearn_steps_mins
    } else {
        &conf.learn_steps_mins
    };
    let total = steps.len();
    let remaining = remaining_steps(card.left);
    let current_step = total.saturating_sub(remaining);

    match rating {
        Rating::Again => {
            // Reset to step 0
            card.left = encode_left(total, total);
            card.due = timing.now_secs + sm2::learning_step_secs(steps, 0);
            card.queue = 1;
        }
        Rating::Hard => {
            // Repeat current step (or average of current and next)
            card.due = timing.now_secs + sm2::learning_step_secs(steps, current_step);
            card.queue = 1;
        }
        Rating::Good => {
            let next_step = current_step + 1;
            if next_step >= total {
                // Graduate
                if is_relearn {
                    // Return to review
                    card.ctype = 2;
                    card.queue = 2;
                    card.due = timing.days_elapsed + card.ivl.max(1) as i64;
                    card.left = 0;
                } else {
                    graduate(card, conf.graduating_interval_good as i32, card.factor, timing);
                }
            } else {
                card.left = encode_left(total - next_step, total - next_step);
                card.due = timing.now_secs + sm2::learning_step_secs(steps, next_step);
                card.queue = 1;
            }
        }
        Rating::Easy => {
            if is_relearn {
                card.ctype = 2;
                card.queue = 2;
                card.due = timing.days_elapsed + card.ivl.max(1) as i64;
                card.left = 0;
            } else {
                graduate(card, conf.graduating_interval_easy as i32, card.factor, timing);
            }
        }
    }
}

fn answer_review(
    card: &mut CardRow,
    rating: Rating,
    conf: &DeckSchedulingConfig,
    timing: &SchedTiming,
) {
    let days_late = (timing.days_elapsed - card.due).max(0) as i32;
    let current_ivl = card.ivl.max(1);

    if rating == Rating::Again {
        // Lapse
        card.lapses += 1;
        let (new_ivl, new_factor) =
            sm2::review_interval(current_ivl, card.factor, days_late, rating, conf);
        card.factor = new_factor;
        card.ivl = new_ivl;

        // Enter relearning
        let relearn_steps = &conf.relearn_steps_mins;
        if relearn_steps.is_empty() {
            // No relearn steps, go straight back to review
            card.queue = 2;
            card.due = timing.days_elapsed + new_ivl as i64;
        } else {
            card.ctype = 3; // relearn
            card.queue = 1;
            card.left = encode_left(relearn_steps.len(), relearn_steps.len());
            card.due = timing.now_secs + sm2::learning_step_secs(relearn_steps, 0);
        }
    } else {
        let (new_ivl, new_factor) =
            sm2::review_interval(current_ivl, card.factor, days_late, rating, conf);
        card.factor = new_factor;
        card.ivl = new_ivl;
        card.queue = 2;
        card.due = timing.days_elapsed + new_ivl as i64;
    }
}

fn graduate(card: &mut CardRow, interval: i32, factor: i32, timing: &SchedTiming) {
    card.ctype = 2;
    card.queue = 2;
    card.ivl = interval;
    card.due = timing.days_elapsed + interval as i64;
    card.factor = factor;
    card.left = 0;
}

fn encode_left(remaining: usize, total_today: usize) -> i32 {
    remaining as i32 + (total_today as i32) * 1000
}

fn remaining_steps(left: i32) -> usize {
    (left % 1000) as usize
}
