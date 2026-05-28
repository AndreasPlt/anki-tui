use crate::db::models::*;
use crate::error::Result;
use rusqlite::Connection;

pub fn load_decks(conn: &Connection) -> Result<Vec<DeckRow>> {
    let mut stmt = conn.prepare("SELECT id, name, kind FROM decks ORDER BY name")?;
    let rows = stmt.query_map([], |row| {
        Ok(DeckRow {
            id: row.get(0)?,
            name: row.get(1)?,
            kind: row.get(2)?,
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn load_deck_configs(conn: &Connection) -> Result<Vec<DeckConfigRow>> {
    let mut stmt = conn.prepare("SELECT id, name, config FROM deck_config")?;
    let rows = stmt.query_map([], |row| {
        Ok(DeckConfigRow {
            id: row.get(0)?,
            name: row.get(1)?,
            config: row.get(2)?,
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Count due cards per deck for the deck selection screen.
pub fn deck_due_counts(
    conn: &Connection,
    deck_id: i64,
    today: i64,
    now_secs: i64,
) -> Result<(u32, u32, u32)> {
    let new: u32 = conn.query_row(
        "SELECT COUNT(*) FROM cards WHERE did = ? AND queue = 0",
        [deck_id],
        |row| row.get(0),
    )?;

    let learn: u32 = conn.query_row(
        "SELECT COUNT(*) FROM cards WHERE did = ? AND (queue = 1 OR queue = 3) AND due <= ?",
        rusqlite::params![deck_id, now_secs],
        |row| row.get(0),
    )?;

    let review: u32 = conn.query_row(
        "SELECT COUNT(*) FROM cards WHERE did = ? AND queue = 2 AND due <= ?",
        rusqlite::params![deck_id, today],
        |row| row.get(0),
    )?;

    Ok((new, learn, review))
}

/// Load due cards for review, ordered by priority.
pub fn load_due_cards(
    conn: &Connection,
    deck_id: i64,
    today: i64,
    now_secs: i64,
    new_limit: u32,
    review_limit: u32,
) -> Result<Vec<CardRow>> {
    // Learning cards (no limit — always show)
    let mut cards = load_cards_query(
        conn,
        "SELECT id, nid, did, ord, mod, usn, type, queue, due, ivl, factor, \
         reps, lapses, left, odue, odid, flags, data \
         FROM cards WHERE did = ? AND (queue = 1 AND due <= ? OR queue = 3 AND due <= ?) \
         ORDER BY due ASC",
        rusqlite::params![deck_id, now_secs, today],
    )?;

    // Review cards
    let reviews = load_cards_query(
        conn,
        "SELECT id, nid, did, ord, mod, usn, type, queue, due, ivl, factor, \
         reps, lapses, left, odue, odid, flags, data \
         FROM cards WHERE did = ? AND queue = 2 AND due <= ? \
         ORDER BY due ASC LIMIT ?",
        rusqlite::params![deck_id, today, review_limit],
    )?;
    cards.extend(reviews);

    // New cards
    let new_cards = load_cards_query(
        conn,
        "SELECT id, nid, did, ord, mod, usn, type, queue, due, ivl, factor, \
         reps, lapses, left, odue, odid, flags, data \
         FROM cards WHERE did = ? AND queue = 0 \
         ORDER BY due ASC LIMIT ?",
        rusqlite::params![deck_id, new_limit],
    )?;
    cards.extend(new_cards);

    Ok(cards)
}

fn load_cards_query(
    conn: &Connection,
    sql: &str,
    params: impl rusqlite::Params,
) -> Result<Vec<CardRow>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params, |row| {
        Ok(CardRow {
            id: row.get(0)?,
            nid: row.get(1)?,
            did: row.get(2)?,
            ord: row.get(3)?,
            mod_: row.get(4)?,
            usn: row.get(5)?,
            ctype: row.get(6)?,
            queue: row.get(7)?,
            due: row.get(8)?,
            ivl: row.get(9)?,
            factor: row.get(10)?,
            reps: row.get(11)?,
            lapses: row.get(12)?,
            left: row.get(13)?,
            odue: row.get(14)?,
            odid: row.get(15)?,
            flags: row.get(16)?,
            data: row.get(17)?,
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn load_note(conn: &Connection, note_id: i64) -> Result<NoteRow> {
    Ok(conn.query_row(
        "SELECT id, mid, flds, tags FROM notes WHERE id = ?",
        [note_id],
        |row| {
            Ok(NoteRow {
                id: row.get(0)?,
                mid: row.get(1)?,
                flds: row.get(2)?,
                tags: row.get(3)?,
            })
        },
    )?)
}

pub fn load_template(conn: &Connection, ntid: i64, ord: i32) -> Result<TemplateRow> {
    Ok(conn.query_row(
        "SELECT ntid, ord, name, config FROM templates WHERE ntid = ? AND ord = ?",
        rusqlite::params![ntid, ord],
        |row| {
            Ok(TemplateRow {
                ntid: row.get(0)?,
                ord: row.get(1)?,
                name: row.get(2)?,
                config: row.get(3)?,
            })
        },
    )?)
}

pub fn load_field_names(conn: &Connection, ntid: i64) -> Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT name FROM fields WHERE ntid = ? ORDER BY ord")?;
    let rows = stmt.query_map([ntid], |row| row.get::<_, String>(0))?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn load_notetype(conn: &Connection, ntid: i64) -> Result<NotetypeRow> {
    Ok(conn.query_row(
        "SELECT id, name, config FROM notetypes WHERE id = ?",
        [ntid],
        |row| {
            Ok(NotetypeRow {
                id: row.get(0)?,
                name: row.get(1)?,
                config: row.get(2)?,
            })
        },
    )?)
}

/// Get collection creation timestamp from the col table.
pub fn get_collection_creation_time(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("SELECT crt FROM col", [], |row| row.get(0))?)
}

/// Get creation UTC offset from config table (stored as ASCII string like "-120").
pub fn get_creation_offset_mins(conn: &Connection) -> Result<i32> {
    let val: Vec<u8> = conn.query_row(
        "SELECT val FROM config WHERE KEY = 'creationOffset'",
        [],
        |row| row.get(0),
    )?;
    let s = std::str::from_utf8(&val).unwrap_or("0");
    Ok(s.parse().unwrap_or(0))
}
