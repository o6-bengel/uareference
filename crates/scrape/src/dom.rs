use quick_xml::events::{
    BytesRef,
    BytesStart,
    Event,
};
use quick_xml::{
    Reader,
    XmlVersion,
};

use crate::error::ScrapeError;

/// A minimal owned XML tree; names keep their source prefixes (`tbx:term`, `xlink:href`).
#[derive(Debug, Default)]
pub struct Element {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
}

#[derive(Debug)]
pub enum Node {
    Element(Element),
    Text(String),
}

impl Node {
    pub fn as_element(&self) -> Option<&Element> {
        match self {
            Node::Element(element) => Some(element),
            Node::Text(_) => None,
        }
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            Node::Text(text) => Some(text),
            Node::Element(_) => None,
        }
    }
}

impl Element {
    pub fn attr(
        &self,
        name: &str,
    ) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(Node::as_element)
    }

    pub fn child(
        &self,
        name: &str,
    ) -> Option<&Element> {
        self.elements().find(|e| e.name == name)
    }

    pub fn children_named<'a>(
        &'a self,
        name: &'a str,
    ) -> impl Iterator<Item = &'a Element> {
        self.elements().filter(move |e| e.name == name)
    }

    pub fn text(&self) -> String {
        let mut out = String::new();
        self.push_text(&mut out);
        out
    }

    fn push_text(
        &self,
        out: &mut String,
    ) {
        for child in &self.children {
            match child {
                Node::Text(text) => out.push_str(text),
                Node::Element(element) => element.push_text(out),
            }
        }
    }

    /// Visits this element and every descendant element in document order.
    pub fn walk<'a>(
        &'a self,
        visit: &mut impl FnMut(&'a Element),
    ) {
        visit(self);
        for element in self.elements() {
            element.walk(visit);
        }
    }
}

pub fn parse(xml: &[u8]) -> Result<Element, ScrapeError> {
    let source = std::str::from_utf8(xml).map_err(|_| ScrapeError::Utf8)?;
    let mut reader = Reader::from_str(source);
    reader.config_mut().trim_text(false);
    let mut stack = vec![Element::default()];
    loop {
        let event = reader
            .read_event()
            .map_err(|e| ScrapeError::Xml(format!("at byte {}: {e}", reader.buffer_position())))?;
        match event {
            Event::Start(start) => stack.push(open(&start)?),
            Event::Empty(start) => append(&mut stack, Node::Element(open(&start)?)),
            Event::End(_) => close(&mut stack)?,
            Event::Text(text) => append_text(&mut stack, &text.xml10_content()),
            Event::CData(data) => append_text(&mut stack, &data.xml10_content()),
            Event::GeneralRef(reference) => append_text(&mut stack, &resolve(&reference)),
            Event::Eof => break,
            _ => {}
        }
    }
    let document = stack
        .pop()
        .ok_or_else(|| ScrapeError::Xml("unbalanced document".into()))?;
    document
        .children
        .into_iter()
        .find_map(|node| match node {
            Node::Element(element) => Some(element),
            Node::Text(_) => None,
        })
        .ok_or_else(|| ScrapeError::Xml("no root element".into()))
}

fn open(start: &BytesStart) -> Result<Element, ScrapeError> {
    let mut attrs = Vec::new();
    for attr in start.attributes() {
        let attr = attr.map_err(|e| ScrapeError::Xml(e.to_string()))?;
        let value = attr
            .normalized_value(XmlVersion::default())
            .map_err(|e| ScrapeError::Xml(e.to_string()))?;
        attrs.push((attr.key.as_ref().to_owned(), value.into_owned()));
    }
    Ok(Element {
        name: start.name().as_ref().to_owned(),
        attrs,
        children: Vec::new(),
    })
}

fn close(stack: &mut Vec<Element>) -> Result<(), ScrapeError> {
    let element = stack
        .pop()
        .ok_or_else(|| ScrapeError::Xml("unexpected end tag".into()))?;
    if stack.is_empty() {
        return Err(ScrapeError::Xml("unexpected end tag".into()));
    }
    append(stack, Node::Element(element));
    Ok(())
}

fn append(
    stack: &mut [Element],
    node: Node,
) {
    if let Some(parent) = stack.last_mut() {
        parent.children.push(node);
    }
}

fn append_text(
    stack: &mut [Element],
    text: &str,
) {
    let Some(parent) = stack.last_mut() else {
        return;
    };
    match parent.children.last_mut() {
        Some(Node::Text(previous)) => previous.push_str(text),
        _ => parent.children.push(Node::Text(text.to_owned())),
    }
}

fn resolve(reference: &BytesRef) -> String {
    if let Ok(Some(c)) = reference.resolve_char_ref() {
        return c.to_string();
    }
    let name = reference.xml10_content();
    match name.as_ref() {
        "amp" => "&".into(),
        "lt" => "<".into(),
        "gt" => ">".into(),
        "quot" => "\"".into(),
        "apos" => "'".into(),
        "nbsp" => "\u{a0}".into(),
        other => format!("&{other};"),
    }
}
