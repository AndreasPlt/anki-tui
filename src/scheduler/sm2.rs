use crate::proto::deck_config::DeckSchedulingConfig;
use rand::Rng;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rating {
    Again = 1,
    Hard = 2,
    Good = 3,
    Easy = 4,
}

impl Rating {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(Rating::Again),
            2 => Some(Rating::Hard),
            3 => Some(Rating::Good),
            4 => Some(Rating::Easy),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Rating::Again => "Again",
            Rating::Hard => "Hard",
            Rating::Good => "Good",
            Rating::Easy => "Easy",
        }
    }
}

/// Compute new interval for a review card.
/// Returns (new_ivl, new_factor).
pub fn review_interval(
    current_ivl: i32,
    factor: i32,
    days_late: i32,
    rating: Rating,
    conf: &DeckSchedulingConfig,
) -> (i32, i32) {
    let ease = factor as f64 / 1000.0;
    let mult = conf.interval_multiplier as f64;
    let max_ivl = conf.maximum_review_interval as i32;

    match rating {
        Rating::Again => {
            let new_ivl = (conf.minimum_lapse_interval as i32).max(1);
            let new_factor = (factor - 200).max(1300);
            (new_ivl, new_factor)
        }
        Rating::Hard => {
            let hard_mult = conf.hard_multiplier as f64;
            let raw = (current_ivl as f64 * hard_mult * mult).round() as i32;
            let new_ivl = fuzz_interval(raw.max(current_ivl + 1).min(max_ivl));
            let new_factor = (factor - 150).max(1300);
            (new_ivl, new_factor)
        }
        Rating::Good => {
            let raw =
                ((current_ivl as f64 + days_late as f64 / 2.0) * ease * mult).round() as i32;
            let new_ivl = fuzz_interval(raw.max(current_ivl + 1).min(max_ivl));
            (new_ivl, factor)
        }
        Rating::Easy => {
            let easy_mult = conf.easy_multiplier as f64;
            let raw = ((current_ivl as f64 + days_late as f64) * ease * easy_mult * mult).round()
                as i32;
            let new_ivl = fuzz_interval(raw.max(current_ivl + 1).min(max_ivl));
            let new_factor = factor + 150;
            (new_ivl, new_factor)
        }
    }
}

/// Preview what interval each rating would give (for display in rating bar).
/// Returns intervals as days (positive) or negative seconds (for learning steps).
pub fn preview_intervals_for_card(
    queue: i32,
    ctype: i32,
    current_ivl: i32,
    factor: i32,
    days_late: i32,
    left: i32,
    conf: &DeckSchedulingConfig,
) -> [i32; 4] {
    match queue {
        // Review cards: use SM-2 interval computation
        2 => [
            Rating::Again,
            Rating::Hard,
            Rating::Good,
            Rating::Easy,
        ]
        .map(|r| review_interval(current_ivl.max(1), factor, days_late, r, conf).0),

        // New cards: show learning step times
        0 => {
            let steps = &conf.learn_steps_mins;
            let step0_secs = learning_step_secs(steps, 0) as i32;
            let step1_secs = steps.get(1).map(|&m| (m * 60.0) as i32);
            [
                -step0_secs,                                               // Again: first step
                -(step0_secs + step1_secs.unwrap_or(step0_secs)) / 2,     // Hard: avg of step 0+1
                step1_secs.map(|s| -s).unwrap_or(                         // Good: next step or graduate
                    conf.graduating_interval_good as i32
                ),
                conf.graduating_interval_easy as i32,                      // Easy: graduate easy
            ]
        }

        // Learning / day-learning / relearning cards
        1 | 3 => {
            let steps = if ctype == 3 {
                &conf.relearn_steps_mins
            } else {
                &conf.learn_steps_mins
            };
            let remaining = (left % 1000) as usize;
            let total = steps.len();
            let current_step = total.saturating_sub(remaining);
            let step_secs = |idx: usize| -> i32 { -(learning_step_secs(steps, idx) as i32) };

            let again = step_secs(0);
            let hard = step_secs(current_step);
            let good = if current_step + 1 >= total {
                // Would graduate
                if ctype == 3 {
                    current_ivl.max(1) // relearn: back to review interval
                } else {
                    conf.graduating_interval_good as i32
                }
            } else {
                step_secs(current_step + 1)
            };
            let easy = if ctype == 3 {
                current_ivl.max(1)
            } else {
                conf.graduating_interval_easy as i32
            };
            [again, hard, good, easy]
        }

        _ => [0; 4],
    }
}

/// Add small random fuzz to interval to spread out reviews.
fn fuzz_interval(ivl: i32) -> i32 {
    if ivl < 3 {
        return ivl;
    }
    let fuzz_range = match ivl {
        3..=7 => 1,
        8..=30 => 2,
        _ => (ivl as f64 * 0.05).round() as i32,
    };
    let mut rng = rand::rng();
    let delta = rng.random_range(-fuzz_range..=fuzz_range);
    (ivl + delta).max(1)
}

/// Get the learning step delay in seconds for a given step index.
pub fn learning_step_secs(steps: &[f32], step_idx: usize) -> i64 {
    let mins = steps.get(step_idx).copied().unwrap_or(1.0);
    (mins * 60.0) as i64
}

/// Format an interval for display (e.g. "1d", "2.5mo", "1y").
pub fn format_interval(ivl: i32) -> String {
    if ivl == 0 {
        return "<1m".to_string();
    }
    if ivl < 0 {
        // Negative = seconds
        let secs = -ivl;
        if secs < 60 {
            format!("{secs}s")
        } else {
            format!("{}m", secs / 60)
        }
    } else if ivl < 30 {
        format!("{ivl}d")
    } else if ivl < 365 {
        format!("{:.1}mo", ivl as f64 / 30.0)
    } else {
        format!("{:.1}y", ivl as f64 / 365.0)
    }
}
