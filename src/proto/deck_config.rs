use super::decode::{decode_fields, get_float32, get_packed_f32s, get_varint};

#[derive(Debug, Clone)]
pub struct DeckSchedulingConfig {
    pub learn_steps_mins: Vec<f32>,
    pub relearn_steps_mins: Vec<f32>,
    pub new_per_day: u32,
    pub reviews_per_day: u32,
    pub initial_ease: f32,
    pub easy_multiplier: f32,
    pub hard_multiplier: f32,
    pub interval_multiplier: f32,
    pub minimum_lapse_interval: u32,
    pub maximum_review_interval: u32,
    pub graduating_interval_good: u32,
    pub graduating_interval_easy: u32,
}

impl Default for DeckSchedulingConfig {
    fn default() -> Self {
        Self {
            learn_steps_mins: vec![1.0, 10.0],
            relearn_steps_mins: vec![10.0],
            new_per_day: 20,
            reviews_per_day: 200,
            initial_ease: 2.5,
            easy_multiplier: 1.3,
            hard_multiplier: 1.2,
            interval_multiplier: 1.0,
            minimum_lapse_interval: 1,
            maximum_review_interval: 36500,
            graduating_interval_good: 1,
            graduating_interval_easy: 4,
        }
    }
}

pub fn decode_deck_config(data: &[u8]) -> DeckSchedulingConfig {
    let fields = decode_fields(data);
    let def = DeckSchedulingConfig::default();

    DeckSchedulingConfig {
        learn_steps_mins: {
            let v = get_packed_f32s(&fields, 1);
            if v.is_empty() { def.learn_steps_mins } else { v }
        },
        relearn_steps_mins: {
            let v = get_packed_f32s(&fields, 2);
            if v.is_empty() { def.relearn_steps_mins } else { v }
        },
        new_per_day: get_varint(&fields, 9).map(|v| v as u32).unwrap_or(def.new_per_day),
        reviews_per_day: get_varint(&fields, 10).map(|v| v as u32).unwrap_or(def.reviews_per_day),
        initial_ease: get_float32(&fields, 11).unwrap_or(def.initial_ease),
        easy_multiplier: get_float32(&fields, 12).unwrap_or(def.easy_multiplier),
        hard_multiplier: get_float32(&fields, 13).unwrap_or(def.hard_multiplier),
        interval_multiplier: get_float32(&fields, 15).unwrap_or(def.interval_multiplier),
        maximum_review_interval: get_varint(&fields, 16).map(|v| v as u32).unwrap_or(def.maximum_review_interval),
        minimum_lapse_interval: get_varint(&fields, 17).map(|v| v as u32).unwrap_or(def.minimum_lapse_interval),
        graduating_interval_good: get_varint(&fields, 18).map(|v| v as u32).unwrap_or(def.graduating_interval_good),
        graduating_interval_easy: get_varint(&fields, 19).map(|v| v as u32).unwrap_or(def.graduating_interval_easy),
    }
}
