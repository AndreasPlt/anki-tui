/// Minimal protobuf wire format decoder.
/// Handles varint, length-delimited (strings/bytes/nested), and fixed32 fields.

#[derive(Debug, Clone)]
pub enum WireValue<'a> {
    Varint(u64),
    Bytes(&'a [u8]),
    Fixed32([u8; 4]),
}

pub struct ProtoReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> ProtoReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn read_varint(&mut self) -> Option<u64> {
        let mut val: u64 = 0;
        let mut shift = 0;
        loop {
            let b = *self.data.get(self.pos)?;
            self.pos += 1;
            val |= ((b & 0x7f) as u64) << shift;
            if b & 0x80 == 0 {
                return Some(val);
            }
            shift += 7;
            if shift >= 64 {
                return None;
            }
        }
    }

    pub fn next_field(&mut self) -> Option<(u32, WireValue<'a>)> {
        if self.pos >= self.data.len() {
            return None;
        }
        let tag = self.read_varint()?;
        let field_num = (tag >> 3) as u32;
        let wire_type = (tag & 0x7) as u8;
        match wire_type {
            0 => {
                let val = self.read_varint()?;
                Some((field_num, WireValue::Varint(val)))
            }
            2 => {
                let len = self.read_varint()? as usize;
                if self.pos + len > self.data.len() {
                    return None;
                }
                let bytes = &self.data[self.pos..self.pos + len];
                self.pos += len;
                Some((field_num, WireValue::Bytes(bytes)))
            }
            5 => {
                if self.pos + 4 > self.data.len() {
                    return None;
                }
                let mut buf = [0u8; 4];
                buf.copy_from_slice(&self.data[self.pos..self.pos + 4]);
                self.pos += 4;
                Some((field_num, WireValue::Fixed32(buf)))
            }
            1 => {
                // fixed64 — skip 8 bytes
                if self.pos + 8 > self.data.len() {
                    return None;
                }
                self.pos += 8;
                // Return as varint with the 64-bit value
                None // skip unknown fixed64 fields
            }
            _ => None, // unknown wire type, stop parsing
        }
    }
}

/// Extract all fields into a vec for convenient access.
pub fn decode_fields(data: &[u8]) -> Vec<(u32, WireValue<'_>)> {
    let mut reader = ProtoReader::new(data);
    let mut fields = Vec::new();
    while let Some(field) = reader.next_field() {
        fields.push(field);
    }
    fields
}

/// Helper: get first string value for a field number.
pub fn get_string(fields: &[(u32, WireValue<'_>)], field_num: u32) -> Option<String> {
    fields.iter().find_map(|(num, val)| {
        if *num == field_num {
            if let WireValue::Bytes(b) = val {
                std::str::from_utf8(b).ok().map(|s| s.to_string())
            } else {
                None
            }
        } else {
            None
        }
    })
}

/// Helper: get first varint value for a field number.
pub fn get_varint(fields: &[(u32, WireValue<'_>)], field_num: u32) -> Option<u64> {
    fields.iter().find_map(|(num, val)| {
        if *num == field_num {
            if let WireValue::Varint(v) = val {
                Some(*v)
            } else {
                None
            }
        } else {
            None
        }
    })
}

/// Helper: get first bytes value for a field number.
pub fn get_bytes<'a>(fields: &[(u32, WireValue<'a>)], field_num: u32) -> Option<&'a [u8]> {
    fields.iter().find_map(|(num, val)| {
        if *num == field_num {
            if let WireValue::Bytes(b) = val {
                Some(*b)
            } else {
                None
            }
        } else {
            None
        }
    })
}

/// Helper: decode packed repeated f32 from a bytes field.
pub fn get_packed_f32s(fields: &[(u32, WireValue<'_>)], field_num: u32) -> Vec<f32> {
    let Some(data) = get_bytes(fields, field_num) else {
        return Vec::new();
    };
    data.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

/// Helper: get first fixed32 as f32.
pub fn get_float32(fields: &[(u32, WireValue<'_>)], field_num: u32) -> Option<f32> {
    fields.iter().find_map(|(num, val)| {
        if *num == field_num {
            match val {
                WireValue::Fixed32(b) => Some(f32::from_le_bytes(*b)),
                _ => None,
            }
        } else {
            None
        }
    })
}
