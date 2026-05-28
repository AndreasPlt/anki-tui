use super::decode::{decode_fields, get_string};
use crate::error::{Error, Result};

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct TemplateConfig {
    pub qfmt: String,
    pub afmt: String,
    pub browser_font: Option<String>,
    pub font_size: Option<u32>,
}

pub fn decode_template_config(data: &[u8]) -> Result<TemplateConfig> {
    let fields = decode_fields(data);
    let qfmt = get_string(&fields, 1)
        .ok_or_else(|| Error::ProtoDecode("missing qfmt (field 1) in template config".into()))?;
    let afmt = get_string(&fields, 2)
        .ok_or_else(|| Error::ProtoDecode("missing afmt (field 2) in template config".into()))?;
    Ok(TemplateConfig {
        qfmt,
        afmt,
        browser_font: get_string(&fields, 6),
        font_size: super::decode::get_varint(&fields, 7).map(|v| v as u32),
    })
}
