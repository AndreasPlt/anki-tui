use ego_tree::NodeRef;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use scraper::{Html, Node};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct RenderedContent {
    pub lines: Vec<Line<'static>>,
    pub images: Vec<ImageRef>,
    pub audio: Vec<AudioRef>,
}

#[derive(Debug, Clone)]
pub struct ImageRef {
    pub path: PathBuf,
    pub line_index: usize,
}

#[derive(Debug, Clone)]
pub struct AudioRef {
    pub path: PathBuf,
}

struct RenderState {
    lines: Vec<Line<'static>>,
    current_spans: Vec<Span<'static>>,
    style_stack: Vec<Style>,
    images: Vec<ImageRef>,
    audio: Vec<AudioRef>,
    media_dir: PathBuf,
}

impl RenderState {
    fn current_style(&self) -> Style {
        self.style_stack.last().copied().unwrap_or_default()
    }

    fn push_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let style = self.current_style();
        for (i, part) in text.split('\n').enumerate() {
            if i > 0 {
                self.flush_line();
            }
            if !part.is_empty() {
                self.current_spans
                    .push(Span::styled(part.to_string(), style));
            }
        }
    }

    fn flush_line(&mut self) {
        let spans = std::mem::take(&mut self.current_spans);
        self.lines.push(Line::from(spans));
    }

    fn push_style(&mut self, style: Style) {
        let merged = self.current_style().patch(style);
        self.style_stack.push(merged);
    }

    fn pop_style(&mut self) {
        self.style_stack.pop();
    }
}

pub fn html_to_lines(html: &str, media_dir: &Path) -> RenderedContent {
    let document = Html::parse_fragment(html);
    let mut state = RenderState {
        lines: Vec::new(),
        current_spans: Vec::new(),
        style_stack: vec![Style::default()],
        images: Vec::new(),
        audio: Vec::new(),
        media_dir: media_dir.to_path_buf(),
    };

    for child in document.tree.root().children() {
        walk_tree(child, &mut state);
    }

    if !state.current_spans.is_empty() {
        state.flush_line();
    }

    RenderedContent {
        lines: state.lines,
        images: state.images,
        audio: state.audio,
    }
}

fn walk_tree(node_ref: NodeRef<'_, Node>, state: &mut RenderState) {
    match node_ref.value() {
        Node::Text(text) => {
            let collapsed: String = text
                .chars()
                .map(|c| if c.is_whitespace() { ' ' } else { c })
                .collect();
            // Extract [sound:filename] references
            process_text_with_sound(&collapsed, state);
        }
        Node::Element(el) => {
            let tag = el.name.local.as_ref();
            match tag {
                "br" => {
                    state.flush_line();
                }
                "hr" => {
                    state.flush_line();
                    let style = Style::default().fg(Color::DarkGray);
                    state
                        .lines
                        .push(Line::from(Span::styled("─".repeat(40), style)));
                }
                "b" | "strong" => {
                    state.push_style(Style::default().add_modifier(Modifier::BOLD));
                    for child in node_ref.children() {
                        walk_tree(child, state);
                    }
                    state.pop_style();
                    return;
                }
                "i" | "em" => {
                    state.push_style(Style::default().add_modifier(Modifier::ITALIC));
                    for child in node_ref.children() {
                        walk_tree(child, state);
                    }
                    state.pop_style();
                    return;
                }
                "u" => {
                    state.push_style(Style::default().add_modifier(Modifier::UNDERLINED));
                    for child in node_ref.children() {
                        walk_tree(child, state);
                    }
                    state.pop_style();
                    return;
                }
                "span" | "font" | "div" => {
                    let style = parse_element_style(el);
                    if tag == "div" {
                        state.flush_line();
                    }
                    state.push_style(style);
                    for child in node_ref.children() {
                        walk_tree(child, state);
                    }
                    state.pop_style();
                    if tag == "div" {
                        state.flush_line();
                    }
                    return;
                }
                "p" => {
                    state.flush_line();
                    for child in node_ref.children() {
                        walk_tree(child, state);
                    }
                    state.flush_line();
                    state.lines.push(Line::default());
                    return;
                }
                "ul" | "ol" => {
                    state.flush_line();
                    let mut idx = 0;
                    for child in node_ref.children() {
                        if let Node::Element(child_el) = child.value()
                            && child_el.name.local.as_ref() == "li"
                        {
                            idx += 1;
                            let prefix = if tag == "ol" {
                                format!(" {idx}. ")
                            } else {
                                " - ".to_string()
                            };
                            state.push_text(&prefix);
                            for li_child in child.children() {
                                walk_tree(li_child, state);
                            }
                            state.flush_line();
                        }
                    }
                    return;
                }
                "img" => {
                    if let Some(src) = el.attr("src") {
                        let path = media_path(&state.media_dir, src);
                        let line_idx = state.lines.len();
                        state.flush_line();
                        state.images.push(ImageRef {
                            path,
                            line_index: line_idx,
                        });
                        let style = Style::default().fg(Color::Cyan);
                        state
                            .lines
                            .push(Line::from(Span::styled(format!("[img: {src}]"), style)));
                    }
                    return;
                }
                "a" => {
                    state.push_style(
                        Style::default()
                            .add_modifier(Modifier::UNDERLINED)
                            .fg(Color::Blue),
                    );
                    for child in node_ref.children() {
                        walk_tree(child, state);
                    }
                    state.pop_style();
                    return;
                }
                "table" => {
                    state.flush_line();
                    for child in node_ref.children() {
                        walk_tree(child, state);
                    }
                    state.flush_line();
                    return;
                }
                "tr" => {
                    for child in node_ref.children() {
                        walk_tree(child, state);
                        state.push_text("  ");
                    }
                    state.flush_line();
                    return;
                }
                "td" | "th" => {
                    if tag == "th" {
                        state.push_style(Style::default().add_modifier(Modifier::BOLD));
                    }
                    for child in node_ref.children() {
                        walk_tree(child, state);
                    }
                    if tag == "th" {
                        state.pop_style();
                    }
                    return;
                }
                "style" | "script" => {
                    return;
                }
                _ => {}
            }

            for child in node_ref.children() {
                walk_tree(child, state);
            }
        }
        _ => {}
    }
}

/// Process text, extracting `[sound:filename]` references into AudioRef entries.
fn process_text_with_sound(text: &str, state: &mut RenderState) {
    let mut remaining = text;
    while let Some(start) = remaining.find("[sound:") {
        // Push text before the sound tag
        if start > 0 {
            state.push_text(&remaining[..start]);
        }
        let after = &remaining[start + 7..]; // skip "[sound:"
        if let Some(end) = after.find(']') {
            let filename = &after[..end];
            let path = media_path(&state.media_dir, filename);
            state.audio.push(AudioRef { path });
            // Show a styled placeholder
            let style = Style::default().fg(Color::DarkGray);
            state
                .current_spans
                .push(Span::styled(format!("[audio: {filename}]"), style));
            remaining = &after[end + 1..];
        } else {
            // Malformed — push rest as text
            state.push_text(&remaining[start..]);
            return;
        }
    }
    if !remaining.is_empty() {
        state.push_text(remaining);
    }
}

fn media_path(media_dir: &Path, raw: &str) -> PathBuf {
    let raw = raw.strip_prefix("file://").unwrap_or(raw);
    let decoded = percent_decode(raw);
    let path = PathBuf::from(decoded);
    if path.is_absolute() {
        path
    } else {
        media_dir.join(path)
    }
}

fn percent_decode(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (hex_value(bytes[i + 1]), hex_value(bytes[i + 2]))
        {
            out.push((hi << 4) | lo);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| raw.to_string())
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn parse_element_style(el: &scraper::node::Element) -> Style {
    let mut style = Style::default();

    if let Some(color_attr) = el.attr("color")
        && let Some(color) = parse_color(color_attr)
    {
        style = style.fg(color);
    }

    if let Some(style_attr) = el.attr("style") {
        for prop in style_attr.split(';') {
            let prop = prop.trim();
            if let Some((key, value)) = prop.split_once(':') {
                let key = key.trim().to_lowercase();
                let value = value.trim();
                match key.as_str() {
                    "color" => {
                        if let Some(c) = parse_color(value) {
                            style = style.fg(c);
                        }
                    }
                    "background-color" | "background" => {
                        if let Some(c) = parse_color(value) {
                            style = style.bg(c);
                        }
                    }
                    "font-weight" => {
                        if value == "bold" || value == "700" || value == "800" || value == "900" {
                            style = style.add_modifier(Modifier::BOLD);
                        }
                    }
                    "font-style" => {
                        if value == "italic" {
                            style = style.add_modifier(Modifier::ITALIC);
                        }
                    }
                    "text-decoration" => {
                        if value.contains("underline") {
                            style = style.add_modifier(Modifier::UNDERLINED);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    if let Some(classes) = el.attr("class")
        && classes.split_whitespace().any(|c| c == "cloze")
    {
        style = style.fg(Color::Blue).add_modifier(Modifier::BOLD);
    }

    style
}

fn parse_color(s: &str) -> Option<Color> {
    let s = s.trim().to_lowercase();

    match s.as_str() {
        "red" => return Some(Color::Red),
        "green" => return Some(Color::Green),
        "blue" => return Some(Color::Blue),
        "yellow" => return Some(Color::Yellow),
        "cyan" => return Some(Color::Cyan),
        "magenta" => return Some(Color::Magenta),
        "white" => return Some(Color::White),
        "black" => return Some(Color::Black),
        "gray" | "grey" => return Some(Color::Gray),
        "orange" => return Some(Color::Rgb(255, 165, 0)),
        "purple" => return Some(Color::Rgb(128, 0, 128)),
        "brown" => return Some(Color::Rgb(139, 69, 19)),
        "pink" => return Some(Color::Rgb(255, 192, 203)),
        _ => {}
    }

    if let Some(hex) = s.strip_prefix('#') {
        return match hex.len() {
            3 => {
                let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
                let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
                let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
                Some(Color::Rgb(r, g, b))
            }
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                Some(Color::Rgb(r, g, b))
            }
            _ => None,
        };
    }

    if s.starts_with("rgb(") && s.ends_with(')') {
        let inner = &s[4..s.len() - 1];
        let parts: Vec<&str> = inner.split(',').collect();
        if parts.len() == 3 {
            let r = parts[0].trim().parse().ok()?;
            let g = parts[1].trim().parse().ok()?;
            let b = parts[2].trim().parse().ok()?;
            return Some(Color::Rgb(r, g, b));
        }
    }

    None
}
