pub mod html;
pub mod layout;
pub mod net;
pub mod style;

/// Text nodes carry the string and the tag context that styled it.
#[derive(Debug, Clone)]
pub enum BoxContent {
    Text(String, FontSpec),
    Rule,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BoxKind {
    Block,
    InlineText,
    Rule,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FontSpec {
    pub size: f32,
    pub weight: u32,
}

impl Default for FontSpec {
    fn default() -> Self {
        Self { size: 14.0, weight: 400 }
    }
}

#[derive(Debug, Clone)]
pub struct LaidOutBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub kind: BoxKind,
    pub content: BoxContent,
    pub link: Option<String>,
    pub color: Option<(u8, u8, u8)>,
}
