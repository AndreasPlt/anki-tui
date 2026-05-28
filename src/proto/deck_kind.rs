use super::decode::{decode_fields, get_bytes, get_varint};

#[derive(Debug, Clone)]
pub enum DeckKind {
    Normal { config_id: i64 },
    Filtered,
}

pub fn decode_deck_kind(data: &[u8]) -> DeckKind {
    let fields = decode_fields(data);
    // Field 1 = NormalDeck (nested message), Field 2 = FilteredDeck
    if let Some(normal_bytes) = get_bytes(&fields, 1) {
        let inner = decode_fields(normal_bytes);
        let config_id = get_varint(&inner, 1).unwrap_or(1) as i64;
        DeckKind::Normal { config_id }
    } else if get_bytes(&fields, 2).is_some() {
        DeckKind::Filtered
    } else {
        // Default to normal with default config
        DeckKind::Normal { config_id: 1 }
    }
}
