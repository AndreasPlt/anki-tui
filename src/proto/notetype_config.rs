use super::decode::{decode_fields, get_string, get_varint};

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct NotetypeConfig {
    /// 0 = normal, 1 = cloze
    pub kind: u32,
    pub css: String,
    pub latex_pre: Option<String>,
    pub latex_post: Option<String>,
}

pub fn decode_notetype_config(data: &[u8]) -> NotetypeConfig {
    let fields = decode_fields(data);
    NotetypeConfig {
        kind: get_varint(&fields, 1).unwrap_or(0) as u32,
        css: get_string(&fields, 3).unwrap_or_default(),
        latex_pre: get_string(&fields, 5),
        latex_post: get_string(&fields, 6),
    }
}
