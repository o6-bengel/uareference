use uareference_model::{
    Inline,
    Link,
};

use super::Builder;
use super::citations::{
    section_after,
    spec_of_citation,
};
use crate::dom::{
    Element,
    Node,
};

const BLOCK_ELEMENTS: &[&str] = &[
    "p",
    "list",
    "table-wrap",
    "table",
    "fig",
    "note",
    "non-normative-note",
    "code",
    "ref-list",
    "ref",
    "preformat",
    "disp-quote",
    "boxed-text",
];

pub fn is_block(element: &Element) -> bool {
    BLOCK_ELEMENTS.contains(&element.name.as_str())
}

impl Builder<'_> {
    /// Normalized inline content of a node run, whitespace collapsed like HTML renders it.
    pub fn inlines(
        &self,
        nodes: &[Node],
    ) -> Vec<Inline> {
        let mut inlines = self.raw_inlines(nodes);
        normalize(&mut inlines);
        inlines
    }

    fn raw_inlines(
        &self,
        nodes: &[Node],
    ) -> Vec<Inline> {
        let mut out = Vec::new();
        for (i, node) in nodes.iter().enumerate() {
            match node {
                Node::Text(text) => out.push(Inline::Text(collapse_whitespace(text))),
                Node::Element(element) => {
                    let following = nodes.get(i + 1).and_then(Node::as_text).unwrap_or_default();
                    self.push_inline(element, following, &mut out);
                }
            }
        }
        out
    }

    fn push_inline(
        &self,
        element: &Element,
        following: &str,
        out: &mut Vec<Inline>,
    ) {
        let children = || self.raw_inlines(&element.children);
        let inline = match element.name.as_str() {
            "italic" => Inline::Italic(children()),
            "bold" => Inline::Bold(children()),
            "underline" => Inline::Underline(children()),
            "strike" => Inline::Strike(children()),
            "sup" => Inline::Sup(children()),
            "sub" => Inline::Sub(children()),
            "monospace" | "code" => Inline::Mono(children()),
            "break" => Inline::Break,
            "inline-graphic" | "graphic" => Inline::Image {
                src: element.attr("xlink:href").unwrap_or_default().to_owned(),
            },
            "ext-link" | "uri" => self.url_link(element),
            "xref" => match self.xref_target(element, following) {
                Some(target) => Inline::Link {
                    target,
                    content: children(),
                },
                None => return out.extend(children()),
            },
            "std-ref" => match self.std_ref_target(element, following) {
                Some(target) => Inline::Link {
                    target,
                    content: children(),
                },
                None => return out.extend(children()),
            },
            "label" | "title" | "sec-meta" => return,
            _ => return out.extend(children()),
        };
        out.push(inline);
    }

    fn url_link(
        &self,
        element: &Element,
    ) -> Inline {
        let text = element.text();
        let url = element
            .attr("xlink:href")
            .map_or_else(|| text.trim().to_owned(), str::to_owned);
        let content = match self.raw_inlines(&element.children) {
            content if content.is_empty() => vec![Inline::Text(url.clone())],
            content => content,
        };
        Inline::Link {
            target: Link::Url { url },
            content,
        }
    }

    fn xref_target(
        &self,
        element: &Element,
        following: &str,
    ) -> Option<Link> {
        let rid = element.attr("rid").filter(|rid| !rid.is_empty())?;
        if element.attr("ref-type") != Some("other") {
            return Some(Link::Anchor { id: rid.to_owned() });
        }
        let text = element.text();
        let spec = match self.citations.get(rid) {
            Some(citation) => citation.spec.clone(),
            None => spec_of_citation(&text),
        };
        match spec {
            Some(spec) => Some(spec_link(spec, &text, following)),
            None => Some(Link::Anchor { id: rid.to_owned() }),
        }
    }

    fn std_ref_target(
        &self,
        element: &Element,
        following: &str,
    ) -> Option<Link> {
        let text = element.text();
        spec_of_citation(&text).map(|spec| spec_link(spec, &text, following))
    }
}

fn spec_link(
    spec: String,
    own_text: &str,
    following: &str,
) -> Link {
    let own_section = own_text
        .find(',')
        .and_then(|comma| section_after(&own_text[comma..]));
    Link::Spec {
        spec,
        section: own_section.or_else(|| section_after(following)),
    }
}

pub fn collapse_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_space = false;
    for c in text.chars() {
        if matches!(c, '\u{200B}'..='\u{200D}' | '\u{2060}' | '\u{FEFF}') {
            continue;
        }
        if matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{A0}') {
            if !in_space {
                out.push(' ');
            }
            in_space = true;
        } else {
            out.push(c);
            in_space = false;
        }
    }
    out
}

pub fn normalize(inlines: &mut Vec<Inline>) {
    let mut after_space = true;
    squeeze(inlines, &mut after_space);
    trim_end(inlines);
    tidy(inlines);
}

fn squeeze(
    inlines: &mut [Inline],
    after_space: &mut bool,
) {
    for inline in inlines {
        match inline {
            Inline::Text(text) => {
                if *after_space {
                    *text = text.trim_start_matches(' ').to_owned();
                }
                if !text.is_empty() {
                    *after_space = text.ends_with(' ');
                }
            }
            Inline::Break => *after_space = true,
            Inline::Image { .. } => *after_space = false,
            _ => {
                if let Some(children) = children_mut(inline) {
                    squeeze(children, after_space);
                }
            }
        }
    }
}

fn trim_end(inlines: &mut Vec<Inline>) -> bool {
    while let Some(last) = inlines.last_mut() {
        let emptied = match last {
            Inline::Text(text) => {
                let trimmed = text.trim_end_matches(' ');
                text.truncate(trimmed.len());
                text.is_empty()
            }
            Inline::Break => true,
            Inline::Image { .. } => return false,
            _ => children_mut(last).is_some_and(trim_end),
        };
        if !emptied {
            return false;
        }
        inlines.pop();
    }
    true
}

/// Drops empty text, unwraps whitespace-only formatting, and merges adjacent text runs.
fn tidy(inlines: &mut Vec<Inline>) {
    let mut out: Vec<Inline> = Vec::with_capacity(inlines.len());
    for mut inline in inlines.drain(..) {
        if let Some(children) = children_mut(&mut inline) {
            tidy(children);
        }
        match inline {
            Inline::Text(text) if text.is_empty() => {}
            Inline::Text(text) => match out.last_mut() {
                Some(Inline::Text(previous)) => previous.push_str(&text),
                _ => out.push(Inline::Text(text)),
            },
            inline if is_blank_formatting(&inline) => {
                let mut children = into_children(inline);
                tidy(&mut children);
                for child in children {
                    match (out.last_mut(), child) {
                        (Some(Inline::Text(previous)), Inline::Text(text)) => previous.push_str(&text),
                        (_, child) => out.push(child),
                    }
                }
            }
            inline => out.push(inline),
        }
    }
    *inlines = out;
}

fn is_blank_formatting(inline: &Inline) -> bool {
    match inline {
        Inline::Italic(children)
        | Inline::Bold(children)
        | Inline::Underline(children)
        | Inline::Strike(children)
        | Inline::Sup(children)
        | Inline::Sub(children)
        | Inline::Mono(children) => children
            .iter()
            .all(|c| matches!(c, Inline::Text(t) if t.trim().is_empty())),
        _ => false,
    }
}

fn into_children(inline: Inline) -> Vec<Inline> {
    match inline {
        Inline::Italic(children)
        | Inline::Bold(children)
        | Inline::Underline(children)
        | Inline::Strike(children)
        | Inline::Sup(children)
        | Inline::Sub(children)
        | Inline::Mono(children)
        | Inline::Link { content: children, .. } => children,
        _ => Vec::new(),
    }
}

fn children_mut(inline: &mut Inline) -> Option<&mut Vec<Inline>> {
    match inline {
        Inline::Italic(children)
        | Inline::Bold(children)
        | Inline::Underline(children)
        | Inline::Strike(children)
        | Inline::Sup(children)
        | Inline::Sub(children)
        | Inline::Mono(children)
        | Inline::Link { content: children, .. } => Some(children),
        _ => None,
    }
}
