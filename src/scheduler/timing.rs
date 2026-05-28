use chrono::{Local, TimeZone, Timelike};

#[derive(Debug, Clone, Copy)]
pub struct SchedTiming {
    pub now_secs: i64,
    pub days_elapsed: i64,
    #[allow(dead_code)]
    pub next_day_at: i64,
}

/// Calculate Anki's "today" day number and related timing.
///
/// Anki counts days since collection creation, with rollover at 4am local time.
pub fn sched_timing_today(creation_secs: i64, _creation_offset_mins: i32) -> SchedTiming {
    let now = Local::now();
    let now_secs = now.timestamp();

    // Anki rolls over at 4:00 AM local time
    let rollover_hour = 4u32;

    // If before rollover hour, we're still on "yesterday"
    let effective_date = if now.hour() < rollover_hour {
        now.date_naive() - chrono::Duration::days(1)
    } else {
        now.date_naive()
    };

    // Creation date (using local timezone)
    let creation_local = Local
        .timestamp_opt(creation_secs, 0)
        .single()
        .unwrap_or_else(Local::now);
    let creation_date = if creation_local.hour() < rollover_hour {
        creation_local.date_naive() - chrono::Duration::days(1)
    } else {
        creation_local.date_naive()
    };

    let days_elapsed = (effective_date - creation_date).num_days();

    // Next rollover: today (or tomorrow) at rollover_hour
    let today_rollover = effective_date + chrono::Duration::days(1);
    let next_day_at = Local
        .from_local_datetime(
            &today_rollover
                .and_hms_opt(rollover_hour, 0, 0)
                .unwrap_or_default(),
        )
        .single()
        .map(|dt| dt.timestamp())
        .unwrap_or(now_secs + 86400);

    SchedTiming {
        now_secs,
        days_elapsed,
        next_day_at,
    }
}
