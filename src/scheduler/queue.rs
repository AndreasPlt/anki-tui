use crate::db::models::{CardRow, DeckInfo, DeckRow};
use crate::db::queries;
use crate::error::Result;
use crate::proto::deck_config::DeckSchedulingConfig;
use crate::proto::deck_kind::{self, DeckKind};
use crate::scheduler::timing::SchedTiming;
use rusqlite::Connection;
use std::collections::HashMap;

/// Build the list of decks with their due counts.
pub fn build_deck_list(
    conn: &Connection,
    timing: &SchedTiming,
    deck_rows: &[DeckRow],
) -> Result<Vec<DeckInfo>> {
    let mut infos = Vec::new();
    for deck in deck_rows {
        let (new, learn, review) =
            queries::deck_due_counts(conn, deck.id, timing.days_elapsed, timing.now_secs)?;
        infos.push(DeckInfo {
            id: deck.id,
            name: deck.name.replace('\x1f', "::"),
            new_count: new,
            learn_count: learn,
            review_count: review,
        });
    }
    Ok(infos)
}

/// Load the scheduling config for a deck by resolving its config_id.
pub fn get_deck_scheduling_config(
    conn: &Connection,
    deck: &DeckRow,
) -> Result<DeckSchedulingConfig> {
    let kind = deck_kind::decode_deck_kind(&deck.kind);
    let config_id = match kind {
        DeckKind::Normal { config_id } => config_id,
        DeckKind::Filtered => return Ok(DeckSchedulingConfig::default()),
    };

    let configs = queries::load_deck_configs(conn)?;
    let config_map: HashMap<i64, _> = configs.into_iter().map(|c| (c.id, c)).collect();

    if let Some(dc) = config_map.get(&config_id) {
        Ok(crate::proto::deck_config::decode_deck_config(&dc.config))
    } else {
        Ok(DeckSchedulingConfig::default())
    }
}

/// Load the review queue for a deck.
pub fn load_review_queue(
    conn: &Connection,
    deck_id: i64,
    timing: &SchedTiming,
    conf: &DeckSchedulingConfig,
) -> Result<Vec<CardRow>> {
    queries::load_due_cards(
        conn,
        deck_id,
        timing.days_elapsed,
        timing.now_secs,
        conf.new_per_day,
        conf.reviews_per_day,
    )
}
