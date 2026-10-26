//! Парсинг HTML (html5ever) в упрощённое DOM-дерево `DomNode`.

use std::collections::HashMap;

use html5ever::parse_document;
use html5ever::tree_builder::TreeSink;
use markup5ever_rcdom::{Handle, NodeData, RcDom};

#[derive(Debug, Clone)]
pub struct DomNode {
    pub tag: String,
    pub attrs: HashMap<String, String>,
    pub children: Vec<DomNode>,
    pub text: String,
}

impl DomNode {
    fn element(tag: &str, attrs: HashMap<String, String>) -> Self {
        Self { tag: tag.to_string(), attrs, children: Vec::new(), text: String::new() }
    }

    fn text_node(text: &str) -> Self {
        Self { tag: String::new(), attrs: HashMap::new(), children: Vec::new(), text: text.to_string() }
    }

    /// Первое значение атрибута по цепочке предков не ищем — только сам узел.
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.get(name).map(String::as_str)
    }
}

/// Разбирает HTML-документ; не падает на битой разметке (html5ever восстановляет).
pub fn parse(html: &str) -> DomNode {
    let dom: RcDom = parse_document(RcDom::default(), Default::default())
        .from_utf8()
        .read_from(&mut html.as_bytes())
        .expect("html5ever never fails on input, only recovers");
    convert(&dom.document)
}

fn convert(handle: &Handle) -> DomNode {
    match &handle.data {
        NodeData::Text { contents } => DomNode::text_node(&contents.borrow().clone()),
        NodeData::Element { name, attrs, .. } => {
            let attrs = attrs
                .borrow()
                .iter()
                .map(|a| (a.name.local.to_string(), a.value.to_string()))
                .collect();
            let mut node = DomNode::element(name.local.as_ref(), attrs);
            for child in handle.children.borrow().iter() {
                node.children.push(convert(child));
            }
            node
        }
        NodeData::Document => {
            let mut node = DomNode::element("#document", HashMap::new());
            for child in handle.children.borrow().iter() {
                node.children.push(convert(child));
            }
            node
        }
        _ => {
            let mut node = DomNode::element("", HashMap::new());
            for child in handle.children.borrow().iter() {
                node.children.push(convert(child));
            }
            node
        }
    }
}

/// Заголовок <title> из дерева.
pub fn title_of(root: &DomNode) -> Option<String> {
    if root.tag == "title" {
        let t = collect_text(root);
        let t = t.trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }
    for child in &root.children {
        if let Some(t) = title_of(child) {
            return Some(t);
        }
    }
    None
}

/// Весь видимый текст под узлом (script/style пропускаются, блоки дают перенос строки).
pub fn visible_text(root: &DomNode, out: &mut String) {
    match root.tag.as_str() {
        "script" | "style" | "head" => return,
        "p" | "div" | "br" | "li" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
            out.push('\n');
        }
        _ => {}
    }
    if !root.text.is_empty() {
        out.push_str(&root.text);
    }
    for child in &root.children {
        visible_text(child, out);
    }
}

fn collect_text(node: &DomNode) -> String {
    let mut s = String::new();
    if !node.text.is_empty() {
        s.push_str(&node.text);
    }
    for child in &node.children {
        s.push_str(&collect_text(child));
    }
    s
}
