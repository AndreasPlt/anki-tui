use std::collections::HashMap;

/// Render an Anki template string by substituting fields.
///
/// Supports: {{Field}}, {{{Field}}} (raw), {{#Field}}...{{/Field}} (conditional),
/// {{^Field}}...{{/Field}} (inverse), {{FrontSide}}, {{cloze:Field}}.
pub fn render_template(
    template: &str,
    fields: &HashMap<String, String>,
    front_side: Option<&str>,
    card_ord: i32,
) -> String {
    let mut result = template.to_string();

    // Handle {{FrontSide}} substitution
    if let Some(front) = front_side {
        result = result.replace("{{FrontSide}}", front);
    }

    // Handle conditionals {{#Field}}...{{/Field}} and {{^Field}}...{{/Field}}
    result = process_conditionals(&result, fields);

    // Handle cloze deletions {{cloze:Field}}
    result = process_cloze(&result, fields, card_ord);

    // Handle triple-brace (raw) substitutions {{{Field}}}
    for (name, value) in fields {
        let tag = format!("{{{{{{{name}}}}}}}");
        result = result.replace(&tag, value);
    }

    // Handle double-brace substitutions {{Field}}
    // Anki inserts raw HTML for both {{Field}} and {{{Field}}}
    for (name, value) in fields {
        let tag = format!("{{{{{name}}}}}");
        if !tag.starts_with("{{cloze:") && !tag.starts_with("{{type:") {
            result = result.replace(&tag, value);
        }
    }

    // Handle {{type:Field}} — render as plain text hint
    for (name, _value) in fields {
        let tag = format!("{{{{type:{name}}}}}");
        result = result.replace(&tag, "<i>[Type answer]</i>");
    }

    result
}

fn process_conditionals(template: &str, fields: &HashMap<String, String>) -> String {
    let mut result = template.to_string();

    // Process {{#Field}}...{{/Field}}
    loop {
        let Some(start) = result.find("{{#") else {
            break;
        };
        let Some(name_end) = result[start + 3..].find("}}") else {
            break;
        };
        let field_name = &result[start + 3..start + 3 + name_end];
        let close_tag = format!("{{{{/{field_name}}}}}");
        let Some(close_pos) = result.find(&close_tag) else {
            break;
        };

        let inner = &result[start + 3 + name_end + 2..close_pos];
        let is_non_empty = fields
            .get(field_name)
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false);

        let replacement = if is_non_empty {
            inner.to_string()
        } else {
            String::new()
        };
        result = format!(
            "{}{}{}",
            &result[..start],
            replacement,
            &result[close_pos + close_tag.len()..]
        );
    }

    // Process {{^Field}}...{{/Field}} (inverse)
    loop {
        let Some(start) = result.find("{{^") else {
            break;
        };
        let Some(name_end) = result[start + 3..].find("}}") else {
            break;
        };
        let field_name = &result[start + 3..start + 3 + name_end];
        let close_tag = format!("{{{{/{field_name}}}}}");
        let Some(close_pos) = result.find(&close_tag) else {
            break;
        };

        let inner = &result[start + 3 + name_end + 2..close_pos];
        let is_empty = fields
            .get(field_name)
            .map(|v| v.trim().is_empty())
            .unwrap_or(true);

        let replacement = if is_empty {
            inner.to_string()
        } else {
            String::new()
        };
        result = format!(
            "{}{}{}",
            &result[..start],
            replacement,
            &result[close_pos + close_tag.len()..]
        );
    }

    result
}

fn process_cloze(template: &str, fields: &HashMap<String, String>, card_ord: i32) -> String {
    let mut result = template.to_string();
    let cloze_num = card_ord + 1; // Anki cloze numbers are 1-indexed, card ord is 0-indexed

    for (name, value) in fields {
        let tag = format!("{{{{cloze:{name}}}}}");
        if result.contains(&tag) {
            let rendered = render_cloze_field(value, cloze_num);
            result = result.replace(&tag, &rendered);
        }
    }

    result
}

/// Render cloze deletions in a field value.
/// {{c1::answer::hint}} → for matching cloze: show [...] on front or answer on back
/// For non-matching cloze numbers: show the answer text.
fn render_cloze_field(field: &str, active_num: i32) -> String {
    let mut result = String::new();
    let mut pos = 0;
    let bytes = field.as_bytes();

    while pos < bytes.len() {
        if field[pos..].starts_with("{{c") {
            if let Some(parsed) = parse_cloze_deletion(&field[pos..]) {
                if parsed.num == active_num {
                    // Active cloze: show as deletion marker
                    result.push_str("<span class=\"cloze\">[");
                    if let Some(hint) = &parsed.hint {
                        result.push_str(hint);
                    } else {
                        result.push_str("...");
                    }
                    result.push_str("]</span>");
                } else {
                    // Inactive cloze: show answer text
                    result.push_str(&parsed.answer);
                }
                pos += parsed.len;
                continue;
            }
        }
        result.push(bytes[pos] as char);
        pos += 1;
    }

    result
}

struct ClozeMatch {
    num: i32,
    answer: String,
    hint: Option<String>,
    len: usize,
}

fn parse_cloze_deletion(s: &str) -> Option<ClozeMatch> {
    if !s.starts_with("{{c") {
        return None;
    }
    let close = s.find("}}")?;
    let inner = &s[3..close];

    let colon1 = inner.find(':')?;
    let num: i32 = inner[..colon1].parse().ok()?;
    let after_num = &inner[colon1 + 2..]; // skip "::"

    let (answer, hint) = if let Some(colon2) = after_num.find("::") {
        (
            after_num[..colon2].to_string(),
            Some(after_num[colon2 + 2..].to_string()),
        )
    } else {
        (after_num.to_string(), None)
    };

    Some(ClozeMatch {
        num,
        answer,
        hint,
        len: close + 2,
    })
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Build a field map from note fields string and field names.
pub fn build_field_map(flds: &str, field_names: &[String]) -> HashMap<String, String> {
    let values: Vec<&str> = flds.split('\x1f').collect();
    let mut map = HashMap::new();
    for (i, name) in field_names.iter().enumerate() {
        let val = values.get(i).copied().unwrap_or("");
        map.insert(name.clone(), val.to_string());
    }
    map
}
