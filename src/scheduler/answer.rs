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
            let is_relearn = card.ctype == 3;
            answer_learning(&mut new, rating, conf, timing, is_relearn);
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
        last_ivl,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::deck_config::DeckSchedulingConfig;

    fn test_timing() -> SchedTiming {
        SchedTiming {
            now_secs: 1700000000,
            days_elapsed: 1000,
            next_day_at: 1700086400,
        }
    }

    fn test_conf() -> DeckSchedulingConfig {
        DeckSchedulingConfig {
            learn_steps_mins: vec![1.0, 10.0],
            relearn_steps_mins: vec![5.0],
            ..DeckSchedulingConfig::default()
        }
    }

    fn review_card() -> CardRow {
        CardRow {
            id: 1, nid: 1, did: 1, ord: 0, mod_: 0, usn: 0,
            ctype: 2, queue: 2, due: 990, ivl: 30, factor: 2500,
            reps: 5, lapses: 0, left: 0, odue: 0, odid: 0, flags: 0,
            data: String::new(),
        }
    }

    #[test]
    fn review_again_enters_relearning_with_relearn_steps() {
        let conf = test_conf();
        let timing = test_timing();
        let card = review_card();

        let (new_card, revlog) = answer_card(&card, Rating::Again, &conf, &timing, 5000);

        // Card should become relearning
        assert_eq!(new_card.ctype, 3, "ctype should be 3 (relearn)");
        assert_eq!(new_card.queue, 1, "queue should be 1 (learning)");
        assert_eq!(new_card.lapses, 1, "lapses should increment");
        // Due should be now + relearn step (5 min = 300 sec)
        assert_eq!(new_card.due, timing.now_secs + 300);
        // Revlog type should be 2 (relearn)
        assert_eq!(revlog.review_type, 2);
    }

    #[test]
    fn relearning_card_uses_relearn_steps_not_learn_steps() {
        let conf = test_conf();
        let timing = test_timing();
        // A card already in relearning state
        let card = CardRow {
            ctype: 3, queue: 1, due: timing.now_secs - 10,
            ivl: 5, factor: 2300, left: encode_left(1, 1),
            ..review_card()
        };

        let (new_card, _) = answer_card(&card, Rating::Again, &conf, &timing, 3000);

        // Should use relearn step 0 (5 min = 300s), NOT learn step 0 (1 min = 60s)
        assert_eq!(new_card.due, timing.now_secs + 300);
    }

    #[test]
    fn new_card_good_with_two_steps_enters_learning() {
        let conf = test_conf();
        let timing = test_timing();
        let card = CardRow {
            ctype: 0, queue: 0, due: 0, ivl: 0, factor: 0,
            reps: 0, lapses: 0, left: 0,
            ..review_card()
        };

        let (new_card, _) = answer_card(&card, Rating::Good, &conf, &timing, 1000);

        // With 2 learn steps, Good goes to step 1 (10 min = 600s)
        assert_eq!(new_card.ctype, 1, "should be learning");
        assert_eq!(new_card.queue, 1);
        assert_eq!(new_card.due, timing.now_secs + 600);
    }

    #[test]
    fn new_card_easy_graduates_immediately() {
        let conf = test_conf();
        let timing = test_timing();
        let card = CardRow {
            ctype: 0, queue: 0, due: 0, ivl: 0, factor: 0,
            reps: 0, lapses: 0, left: 0,
            ..review_card()
        };

        let (new_card, _) = answer_card(&card, Rating::Easy, &conf, &timing, 1000);

        assert_eq!(new_card.ctype, 2, "should be review");
        assert_eq!(new_card.queue, 2);
        assert_eq!(new_card.ivl, conf.graduating_interval_easy as i32);
    }
}
