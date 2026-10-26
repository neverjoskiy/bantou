//! Фаза 2: бокс-модель — превращает DOM в список `LaidOutBox` (блоки + инлайн-текст).
//!
//! Пока без CSS: типографика берётся из тегов (h1–h6, p, br). Ширина текстовых
//! боксов аппроксимируется по средней ширине символа; точный paint приходит в фазе 3.

use super::html::DomNode;
use super::{BoxContent, BoxKind, FontSpec, LaidOutBox};

/// Средняя ширина глифа как доля от кегля (аппроксимация для переноса).
const AVG_GLYPH_RATIO: f32 = 0.52;

pub fn layout(root: &DomNode, viewport_width: f32) -> Vec<LaidOutBox> {
    let mut ctx = LayoutCtx { boxes: Vec::new(), cursor_y: 0.0, viewport_width };
    for child in &root.children {
        layout_node(child, &mut ctx, FontSpec::default(), None);
    }
    ctx.boxes
}

struct LayoutCtx {
    boxes: Vec<LaidOutBox>,
    cursor_y: f32,
    viewport_width: f32,
}

/// Вертикальные отступы блоков: (до, после).
fn block_spacing(tag: &str) -> (f32, f32) {
    match tag {
        "h1" => (21.0, 21.0),
        "h2" => (19.0, 19.0),
        "h3" => (18.0, 18.0),
        "h4" | "h5" | "h6" => (20.0, 6.0),
        "p" | "ul" | "ol" | "table" | "pre" => (0.0, 16.0),
        "li" => (2.0, 2.0),
        "br" => (0.0, 8.0),
        "hr" => (8.0, 8.0),
        _ => (0.0, 0.0),
    }
}

fn font_for(tag: &str) -> FontSpec {
    match tag {
        "h1" => FontSpec { size: 32.0, weight: 700 },
        "h2" => FontSpec { size: 24.0, weight: 600 },
        "h3" => FontSpec { size: 19.0, weight: 600 },
        "h4" => FontSpec { size: 16.0, weight: 600 },
        "h5" => FontSpec { size: 16.0, weight: 500 },
        "h6" => FontSpec { size: 14.0, weight: 600 },
        "b" | "strong" | "th" => FontSpec { size: 14.0, weight: 700 },
        _ => FontSpec::default(),
    }
}

fn is_block(tag: &str) -> bool {
    matches!(
        tag,
        "p" | "div" | "section" | "article" | "header" | "footer" | "nav" | "main"
            | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "ul" | "ol" | "li" | "table"
            | "pre" | "blockquote" | "figure" | "hr"
    )
}

fn line_height(font: &FontSpec) -> f32 {
    font.size * 1.5
}

/// Разбивает текст на строки, влезающие в `max_width` при среднем кегле.
fn wrap_lines(text: &str, font: &FontSpec, max_width: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut current = String::new();
        for word in paragraph.split_whitespace() {
            let candidate_width =
                (current.len() + 1 + word.len()) as f32 * font.size * AVG_GLYPH_RATIO;
            if !current.is_empty() && candidate_width > max_width {
                lines.push(std::mem::take(&mut current));
            }
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
        }
        lines.push(current);
    }
    lines
}

fn push_text_lines(ctx: &mut LayoutCtx, text: &str, font: FontSpec, inline: bool, link: Option<String>) {
    let max_width = ctx.viewport_width;
    if max_width <= 0.0 {
        return;
    }
    let lines = wrap_lines(text, &font, max_width);
    let height = line_height(&font);
    for line in lines {
        if line.is_empty() {
            continue;
        }
        ctx.boxes.push(LaidOutBox {
            x: 0.0,
            y: ctx.cursor_y,
            width: ctx.viewport_width,
            height,
            kind: if inline { BoxKind::InlineText } else { BoxKind::Block },
            content: BoxContent::Text(line, font),
            link: link.clone(),
        });
        ctx.cursor_y += height;
    }
}

/// Инлайновое содержимое: текст и строчные теги слиянием шрифтов.
fn layout_inline(node: &DomNode, ctx: &mut LayoutCtx, font: FontSpec, link: Option<String>) {
    match node.tag.as_str() {
        "" => {
            if !node.text.is_empty() {
                push_text_lines(ctx, &squash(&node.text), font, true, link);
            }
        }
        "br" => ctx.cursor_y += line_height(&font),
        "img" => {}
        _ => {
            let inner = font_for(&node.tag);
            let merged =
                FontSpec { size: font.size, weight: inner.weight.max(font.weight) };
            let new_link = if node.tag == "a" {
                node.link.clone().or_else(|| node.attrs.get("href").cloned())
            } else {
                link.clone()
            };
            for child in &node.children {
                layout_inline(child, ctx, merged, new_link.clone());
            }
            // text directly inside inline element (if any) already as children text nodes
        }
    }
}

fn layout_node(node: &DomNode, ctx: &mut LayoutCtx, font: FontSpec, link: Option<String>) {
    if node.tag.is_empty() {
        if !node.text.trim().is_empty() {
            push_text_lines(ctx, &squash(&node.text), font, false, link);
        }
        return;
    }
    match node.tag.as_str() {
        "script" | "style" | "head" | "meta" | "link" | "title" => {}
        "hr" => {
            let (before, after) = block_spacing("hr");
            ctx.cursor_y += before;
            ctx.boxes.push(LaidOutBox {
                x: 0.0,
                y: ctx.cursor_y,
                width: ctx.viewport_width,
                height: 1.0,
                kind: BoxKind::Rule,
                content: BoxContent::Rule,
                link: None,
            });
            ctx.cursor_y += 1.0 + after;
        }
        "br" => ctx.cursor_y += line_height(&font),
        t if is_block(t) => {
            let (before, after) = block_spacing(t);
            ctx.cursor_y += before;
            let inner = font_for(t);
            let font = if inner.size > font.size { inner } else { font };
            let new_link = if t == "a" {
                node.link.clone().or_else(|| node.attrs.get("href").cloned())
            } else {
                link.clone()
            };
            for child in &node.children {
                layout_node(child, ctx, font, new_link.clone());
            }
            ctx.cursor_y += after;
        }
        _ => layout_inline(node, ctx, font, link),
    }
}

/// Сжимает пробельные цепочки в один пробел.
fn squash(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last_space = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !last_space {
                out.push(' ');
                last_space = true;
            }
        } else {
            out.push(ch);
            last_space = false;
        }
    }
    out
}
