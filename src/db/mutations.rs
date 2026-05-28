use crate::db::models::{CardRow, RevlogEntry};
use crate::error::Result;
use rusqlite::Connection;

pub fn update_card(conn: &Connection, card: &CardRow) -> Result<()> {
    conn.execute(
        "UPDATE cards SET type=?, queue=?, due=?, ivl=?, factor=?, reps=?, lapses=?, \
         left=?, mod=?, usn=?, odue=?, odid=?, flags=?, data=? WHERE id=?",
        rusqlite::params![
            card.ctype,
            card.queue,
            card.due,
            card.ivl,
            card.factor,
            card.reps,
            card.lapses,
            card.left,
            card.mod_,
            card.usn,
            card.odue,
            card.odid,
            card.flags,
            card.data,
            card.id,
        ],
    )?;
    Ok(())
}

pub fn insert_revlog(conn: &Connection, entry: &RevlogEntry) -> Result<()> {
    conn.execute(
        "INSERT INTO revlog (id, cid, usn, ease, ivl, lastIvl, factor, time, type) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        rusqlite::params![
            entry.id,
            entry.cid,
            entry.usn,
            entry.ease,
            entry.ivl,
            entry.last_ivl,
            entry.factor,
            entry.time,
            entry.review_type,
        ],
    )?;
    Ok(())
}

/// Commit a card update and revlog entry in a single short transaction.
pub fn commit_review(conn: &Connection, card: &CardRow, revlog: &RevlogEntry) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    update_card(&tx, card)?;
    insert_revlog(&tx, revlog)?;
    tx.commit()?;
    Ok(())
}
