/// Schema-mirror structs for Anki's SQLite tables.
/// Fields mirror the DB schema even when not all are read by the app.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct CardRow {
    pub id: i64,
    pub nid: i64,
    pub did: i64,
    pub ord: i32,
    pub mod_: i64,
    pub usn: i32,
    pub ctype: i32,
    pub queue: i32,
    pub due: i64,
    pub ivl: i32,
    pub factor: i32,
    pub reps: i32,
    pub lapses: i32,
    pub left: i32,
    pub odue: i64,
    pub odid: i64,
    pub flags: i32,
    pub data: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct NoteRow {
    pub id: i64,
    pub mid: i64,
    pub flds: String,
    pub tags: String,
}

#[derive(Debug, Clone)]
pub struct DeckRow {
    pub id: i64,
    pub name: String,
    pub kind: Vec<u8>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct TemplateRow {
    pub ntid: i64,
    pub ord: i32,
    pub name: String,
    pub config: Vec<u8>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct NotetypeRow {
    pub id: i64,
    pub name: String,
    pub config: Vec<u8>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct DeckConfigRow {
    pub id: i64,
    pub name: String,
    pub config: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct RevlogEntry {
    pub id: i64,
    pub cid: i64,
    pub usn: i32,
    pub ease: i32,
    pub ivl: i32,
    pub last_ivl: i32,
    pub factor: i32,
    pub time: i32,
    pub review_type: i32,
}

#[derive(Debug, Clone)]
pub struct DeckInfo {
    pub id: i64,
    pub name: String,
    pub new_count: u32,
    pub learn_count: u32,
    pub review_count: u32,
}
