use crate::db::models::{CardRow, DeckInfo, DeckRow};
use crate::db::queries;
use crate::error::Result;
use crate::proto::deck_config::DeckSchedulingConfig;
use crate::proto::deck_kind::{self, DeckKind};
use crate::scheduler::timing::SchedTiming;
use rusqlite::Connection;
use std::collections::HashMap;

/// Build the list of decks with their due counts.
/// Parent decks include counts from all children.
pub fn build_deck_list(
    conn: &Connection,
    timing: &SchedTiming,
    deck_rows: &[DeckRow],
) -> Result<Vec<DeckInfo>> {
    let names: Vec<String> = deck_rows
        .iter()
        .map(|d| d.name.replace('\x1f', "::"))
        .collect();

    let mut infos = Vec::new();
    for (i, deck) in deck_rows.iter().enumerate() {
        let deck_name = &names[i];
        // Collect this deck + all child deck IDs
        let deck_ids = gather_deck_ids(deck.id, deck_name, deck_rows, &names);
        let (new, learn, review) =
            queries::deck_due_counts(conn, &deck_ids, timing.days_elapsed, timing.now_secs)?;
        infos.push(DeckInfo {
            id: deck.id,
            name: deck_name.clone(),
            new_count: new,
            learn_count: learn,
            review_count: review,
        });
    }
    Ok(infos)
}

/// Collect deck IDs for a deck and all its children.
pub fn gather_deck_ids(
    deck_id: i64,
    deck_name: &str,
    deck_rows: &[DeckRow],
    names: &[String],
) -> Vec<i64> {
    let prefix = format!("{deck_name}::");
    let mut ids = vec![deck_id];
    for (i, name) in names.iter().enumerate() {
        if name.starts_with(&prefix) {
            ids.push(deck_rows[i].id);
        }
    }
    ids
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

/// Load the review queue for a deck and all its children.
pub fn load_review_queue(
    conn: &Connection,
    deck_ids: &[i64],
    timing: &SchedTiming,
    conf: &DeckSchedulingConfig,
) -> Result<Vec<CardRow>> {
    queries::load_due_cards(
        conn,
        deck_ids,
        timing.days_elapsed,
        timing.now_secs,
        conf.new_per_day,
        conf.reviews_per_day,
    )
}
