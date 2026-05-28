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

/// Build a SQL `IN (?, ?, ...)` clause for a set of deck IDs.
fn in_clause(ids: &[i64]) -> String {
    let placeholders: Vec<&str> = ids.iter().map(|_| "?").collect();
    format!("({})", placeholders.join(","))
}

/// Bind deck IDs as params starting at a given index.
fn bind_deck_ids(stmt: &mut rusqlite::Statement, ids: &[i64], start: usize) -> rusqlite::Result<()> {
    for (i, id) in ids.iter().enumerate() {
        stmt.raw_bind_parameter(start + i, *id)?;
    }
    Ok(())
}

/// Count due cards across multiple decks.
pub fn deck_due_counts(
    conn: &Connection,
    deck_ids: &[i64],
    today: i64,
    now_secs: i64,
) -> Result<(u32, u32, u32)> {
    if deck_ids.is_empty() {
        return Ok((0, 0, 0));
    }
    let in_cl = in_clause(deck_ids);

    let new: u32 = {
        let sql = format!("SELECT COUNT(*) FROM cards WHERE did IN {in_cl} AND queue = 0");
        let mut stmt = conn.prepare(&sql)?;
        bind_deck_ids(&mut stmt, deck_ids, 1)?;
        stmt.raw_query().next()?.unwrap().get(0)?
    };

    let learn: u32 = {
        let sql = format!(
            "SELECT COUNT(*) FROM cards WHERE did IN {in_cl} AND (queue = 1 OR queue = 3) AND due <= ?"
        );
        let mut stmt = conn.prepare(&sql)?;
        bind_deck_ids(&mut stmt, deck_ids, 1)?;
        stmt.raw_bind_parameter(deck_ids.len() + 1, now_secs)?;
        stmt.raw_query().next()?.unwrap().get(0)?
    };

    let review: u32 = {
        let sql = format!(
            "SELECT COUNT(*) FROM cards WHERE did IN {in_cl} AND queue = 2 AND due <= ?"
        );
        let mut stmt = conn.prepare(&sql)?;
        bind_deck_ids(&mut stmt, deck_ids, 1)?;
        stmt.raw_bind_parameter(deck_ids.len() + 1, today)?;
        stmt.raw_query().next()?.unwrap().get(0)?
    };

    Ok((new, learn, review))
}

const CARD_COLS: &str = "id, nid, did, ord, mod, usn, type, queue, due, ivl, factor, \
    reps, lapses, left, odue, odid, flags, data";

/// Load due cards for review across multiple decks, ordered by priority.
pub fn load_due_cards(
    conn: &Connection,
    deck_ids: &[i64],
    today: i64,
    now_secs: i64,
    new_limit: u32,
    review_limit: u32,
) -> Result<Vec<CardRow>> {
    if deck_ids.is_empty() {
        return Ok(Vec::new());
    }
    let in_cl = in_clause(deck_ids);

    // Learning cards (no limit)
    let mut cards = {
        let sql = format!(
            "SELECT {CARD_COLS} FROM cards \
             WHERE did IN {in_cl} AND (queue = 1 AND due <= ? OR queue = 3 AND due <= ?) \
             ORDER BY due ASC"
        );
        let mut stmt = conn.prepare(&sql)?;
        bind_deck_ids(&mut stmt, deck_ids, 1)?;
        let off = deck_ids.len() + 1;
        stmt.raw_bind_parameter(off, now_secs)?;
        stmt.raw_bind_parameter(off + 1, today)?;
        collect_cards(&mut stmt)?
    };

    // Review cards
    let reviews = {
        let sql = format!(
            "SELECT {CARD_COLS} FROM cards \
             WHERE did IN {in_cl} AND queue = 2 AND due <= ? \
             ORDER BY due ASC LIMIT ?"
        );
        let mut stmt = conn.prepare(&sql)?;
        bind_deck_ids(&mut stmt, deck_ids, 1)?;
        let off = deck_ids.len() + 1;
        stmt.raw_bind_parameter(off, today)?;
        stmt.raw_bind_parameter(off + 1, review_limit)?;
        collect_cards(&mut stmt)?
    };
    cards.extend(reviews);

    // New cards — gather in deck order (sorted by deck name, then position)
    let new_cards = {
        let sql = format!(
            "SELECT {CARD_COLS} FROM cards \
             WHERE did IN {in_cl} AND queue = 0 \
             ORDER BY due ASC LIMIT ?"
        );
        let mut stmt = conn.prepare(&sql)?;
        bind_deck_ids(&mut stmt, deck_ids, 1)?;
        stmt.raw_bind_parameter(deck_ids.len() + 1, new_limit)?;
        collect_cards(&mut stmt)?
    };
    cards.extend(new_cards);

    Ok(cards)
}

fn collect_cards(stmt: &mut rusqlite::Statement) -> Result<Vec<CardRow>> {
    let mut rows = stmt.raw_query();
    let mut cards = Vec::new();
    while let Some(row) = rows.next()? {
        cards.push(CardRow {
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
        });
    }
    Ok(cards)
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
