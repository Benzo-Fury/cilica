//! A tiny XML tree over quick-xml, enough for ENEX and DOCX. Namespaces are dropped: `w:p` is `p`.

use quick_xml::Reader;
use quick_xml::events::{BytesRef, Event};

#[derive(Debug, Default, Clone)]
pub struct Node {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Child>,
}

#[derive(Debug, Clone)]
pub enum Child {
    Element(Node),
    Text(String),
}

impl Node {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }

    pub fn elements(&self) -> impl Iterator<Item = &Node> {
        self.children.iter().filter_map(|c| match c {
            Child::Element(n) => Some(n),
            Child::Text(_) => None,
        })
    }

    pub fn child(&self, name: &str) -> Option<&Node> {
        self.elements().find(|n| n.name == name)
    }

    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Node> + 'a {
        self.elements().filter(move |n| n.name == name)
    }

    /// All text inside, concatenated.
    pub fn text(&self) -> String {
        let mut out = String::new();
        self.collect(&mut out);
        out
    }

    fn collect(&self, out: &mut String) {
        for c in &self.children {
            match c {
                Child::Text(t) => out.push_str(t),
                Child::Element(n) => n.collect(out),
            }
        }
    }

    /// Text of a direct child element, trimmed.
    pub fn child_text(&self, name: &str) -> Option<String> {
        self.child(name).map(|n| n.text().trim().to_string()).filter(|t| !t.is_empty())
    }
}

fn local(name: &str) -> String {
    name.rsplit(':').next().unwrap_or(name).to_string()
}

fn entity(e: &BytesRef) -> String {
    if let Ok(Some(c)) = e.resolve_char_ref() {
        return c.to_string();
    }
    match AsRef::<str>::as_ref(e) {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" => "'",
        "nbsp" => "\u{a0}",
        _ => "",
    }
    .to_string()
}

/// Parse a document into a tree rooted at a synthetic node. Malformed trailing content is ignored.
pub fn parse(xml: &str) -> Node {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut stack: Vec<Node> = vec![Node { name: "#root".into(), ..Default::default() }];
    let start = |e: &quick_xml::events::BytesStart| Node {
        name: local(e.name().as_ref()),
        attrs: e
            .attributes()
            .flatten()
            .map(|a| (local(a.key.as_ref()), a.normalized_value(quick_xml::XmlVersion::Implicit1_0).map(|v| v.to_string()).unwrap_or_default()))
            .collect(),
        children: Vec::new(),
    };
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => stack.push(start(&e)),
            Ok(Event::Empty(e)) => {
                let node = start(&e);
                if let Some(top) = stack.last_mut() {
                    top.children.push(Child::Element(node));
                }
            }
            Ok(Event::End(_)) => {
                if stack.len() > 1 {
                    let node = stack.pop().unwrap_or_default();
                    if let Some(top) = stack.last_mut() {
                        top.children.push(Child::Element(node));
                    }
                }
            }
            Ok(Event::Text(t)) => push_text(&mut stack, &t.html_content()),
            Ok(Event::CData(t)) => push_text(&mut stack, &t.into_inner()),
            Ok(Event::GeneralRef(r)) => push_text(&mut stack, &entity(&r)),
            Ok(Event::Eof) | Err(_) => break,
            Ok(_) => {}
        }
    }
    while stack.len() > 1 {
        let node = stack.pop().unwrap_or_default();
        if let Some(top) = stack.last_mut() {
            top.children.push(Child::Element(node));
        }
    }
    stack.pop().unwrap_or_default()
}

fn push_text(stack: &mut [Node], text: &str) {
    if text.is_empty() {
        return;
    }
    if let Some(top) = stack.last_mut() {
        if let Some(Child::Text(prev)) = top.children.last_mut() {
            prev.push_str(text);
        } else {
            top.children.push(Child::Text(text.to_string()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_entities_cdata_and_namespaces() {
        let root = parse("<?xml version=\"1.0\"?><w:doc a=\"1\"><w:t>Fish &amp; chips &#233;</w:t><c><![CDATA[<b>x</b>]]></c><e/></w:doc>");
        let doc = root.child("doc").unwrap();
        assert_eq!(doc.attr("a"), Some("1"));
        assert_eq!(doc.child_text("t").as_deref(), Some("Fish & chips é"));
        assert_eq!(doc.child_text("c").as_deref(), Some("<b>x</b>"));
        assert!(doc.child("e").is_some());
    }
}
