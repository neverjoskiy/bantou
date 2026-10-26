//! Мини-CSS: селекторы тег/класс/id, специфичность, каскад, наследование.

use crate::engine::html::DomNode;

#[derive(Debug, Clone, Copy, Default)]
pub struct Style {
    pub color: Option<(u8, u8, u8)>,
    pub font_size: Option<f32>,
    pub font_weight: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Selector {
    Universal,
    Tag(String),
    Class(String),
    Id(String),
}

#[derive(Debug, Clone)]
pub struct Rule {
    pub selector: Selector,
    pub specificity: u32,
    pub style: Style,
}

pub fn specificity(sel: &Selector) -> u32 {
    match sel {
        Selector::Id(_) => 100,
        Selector::Class(_) => 10,
        Selector::Tag(_) => 1,
        Selector::Universal => 0,
    }
}

pub fn parse_css(css: &str) -> Vec<Rule> {
    let mut rules = Vec::new();
    for chunk in css.split('}') {
        let Some((sel_part, decl_part)) = chunk.split_once('{') else { continue };
        let selector_str = sel_part.trim();
        if selector_str.is_empty() {
            continue;
        }
        let style = parse_declarations(decl_part);
        for raw_sel in selector_str.split(',') {
            let raw_sel = raw_sel.trim();
            if raw_sel.is_empty() {
                continue;
            }
            if let Some(sel) = parse_selector(raw_sel) {
                rules.push(Rule {
                    specificity: specificity(&sel),
                    selector: sel,
                    style: style.clone(),
                });
            }
        }
    }
    rules
}

fn parse_selector(s: &str) -> Option<Selector> {
    let s = s.trim();
    if s == "*" {
        return Some(Selector::Universal);
    }
    if let Some(id) = s.strip_prefix('#') {
        return Some(Selector::Id(id.to_string()));
    }
    if let Some(cls) = s.strip_prefix('.') {
        return Some(Selector::Class(cls.to_string()));
    }
    if s.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        return Some(Selector::Tag(s.to_lowercase()));
    }
    None
}

pub fn parse_declarations(s: &str) -> Style {
    let mut style = Style::default();
    for decl in s.split(';') {
        let Some((k, v)) = decl.split_once(':') else { continue };
        let key = k.trim().to_lowercase();
        let val = v.trim();
        match key.as_str() {
            "color" => {
                if let Some(c) = parse_color(val) {
                    style.color = Some(c);
                }
            }
            "font-size" => {
                if let Some(n) = parse_length(val) {
                    style.font_size = Some(n);
                }
            }
            "font-weight" => {
                if let Ok(n) = val.trim_end_matches("px").trim().parse::<u32>() {
                    style.font_weight = Some(n);
                }
            }
            _ => {}
        }
    }
    style
}

pub fn parse_inline(style_str: &str) -> Style {
    parse_declarations(style_str)
}

pub fn parse_color(s: &str) -> Option<(u8, u8, u8)> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix('#') {
        return match hex.len() {
            3 => {
                let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?;
                let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?;
                let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?;
                Some((r, g, b))
            }
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                Some((r, g, b))
            }
            _ => None,
        };
    }
    if let Some(rest) = s.strip_prefix("rgb(").and_then(|r| r.strip_suffix(')')) {
        let parts: Vec<&str> = rest.split(',').collect();
        if parts.len() == 3 {
            let r = parts[0].trim().parse::<u8>().ok()?;
            let g = parts[1].trim().parse::<u8>().ok()?;
            let b = parts[2].trim().parse::<u8>().ok()?;
            return Some((r, g, b));
        }
    }
    None
}

fn parse_length(s: &str) -> Option<f32> {
    let s = s.trim();
    let num = s.trim_end_matches("px").trim_end_matches("pt").trim();
    num.parse::<f32>().ok()
}

pub fn matches(sel: &Selector, node: &DomNode) -> bool {
    match sel {
        Selector::Universal => true,
        Selector::Tag(t) => node.tag.eq_ignore_ascii_case(t),
        Selector::Class(c) => node
            .attrs
            .get("class")
            .map(|classes| classes.split_whitespace().any(|x| x == c))
            .unwrap_or(false),
        Selector::Id(i) => node.attrs.get("id").map(|x| x == i).unwrap_or(false),
    }
}

pub fn collect_rules(root: &DomNode) -> Vec<Rule> {
    let mut rules = Vec::new();
    collect_rules_rec(root, &mut rules);
    rules
}

fn collect_rules_rec(node: &DomNode, out: &mut Vec<Rule>) {
    if node.tag.eq_ignore_ascii_case("style") {
        let mut css = String::new();
        collect_text(node, &mut css);
        out.extend(parse_css(&css));
    }
    for c in &node.children {
        collect_rules_rec(c, out);
    }
}

fn collect_text(node: &DomNode, out: &mut String) {
    if !node.text.is_empty() {
        out.push_str(&node.text);
    }
    for c in &node.children {
        collect_text(c, out);
    }
}

pub fn compute_style(node: &DomNode, rules: &[Rule], parent: &Style) -> Style {
    let mut matching: Vec<&Rule> = rules.iter().filter(|r| matches(&r.selector, node)).collect();
    matching.sort_by_key(|r| r.specificity);
    let mut style = *parent;
    for r in matching {
        if r.style.color.is_some() {
            style.color = r.style.color;
        }
        if r.style.font_size.is_some() {
            style.font_size = r.style.font_size;
        }
        if r.style.font_weight.is_some() {
            style.font_weight = r.style.font_weight;
        }
    }
    if let Some(inline) = node.attrs.get("style") {
        let inline_style = parse_inline(inline);
        if inline_style.color.is_some() {
            style.color = inline_style.color;
        }
        if inline_style.font_size.is_some() {
            style.font_size = inline_style.font_size;
        }
        if inline_style.font_weight.is_some() {
            style.font_weight = inline_style.font_weight;
        }
    }
    if style.color.is_none() {
        style.color = parent.color;
    }
    if style.font_size.is_none() {
        style.font_size = parent.font_size;
    }
    if style.font_weight.is_none() {
        style.font_weight = parent.font_weight;
    }
    style
}
EOF
Write-Host done