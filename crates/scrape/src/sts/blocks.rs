use uareference_model::{
    Block,
    Cell,
    Inline,
    Reference,
    Row,
    Table,
};

use super::Builder;
use super::citations::{
    citation_of,
    spec_of_citation,
};
use super::inlines::is_block;
use crate::dom::{
    Element,
    Node,
};

const CAPTION_SEPARATORS: &[&str] = &[" - ", " – ", " — ", " \u{2011} "];

impl Builder<'_> {
    /// Blocks of mixed content: runs of inline nodes become paragraphs, block elements convert.
    pub fn flow(
        &self,
        nodes: &[Node],
    ) -> Vec<Block> {
        let mut blocks = Vec::new();
        let mut run_start = 0;
        for (i, node) in nodes.iter().enumerate() {
            let Some(element) = node.as_element().filter(|e| is_block(e)) else {
                continue;
            };
            self.push_run(&nodes[run_start..i], &mut blocks);
            self.push_block(element, &mut blocks);
            run_start = i + 1;
        }
        self.push_run(&nodes[run_start..], &mut blocks);
        blocks
    }

    pub fn push_run(
        &self,
        nodes: &[Node],
        out: &mut Vec<Block>,
    ) {
        let content = self.inlines(nodes);
        if !content.is_empty() {
            out.push(Block::Paragraph {
                id: None,
                indent: 0,
                content,
            });
        }
    }

    pub fn push_block(
        &self,
        element: &Element,
        out: &mut Vec<Block>,
    ) {
        match element.name.as_str() {
            "p" => out.extend(self.paragraph(element)),
            "list" => out.push(self.list(element)),
            "table-wrap" => out.push(Block::Table(self.table_wrap(element))),
            "table" => out.push(Block::Table(self.table(&[element], None, None, Vec::new()))),
            "fig" => out.push(self.figure(element)),
            "note" | "non-normative-note" => out.push(Block::Note {
                label: None,
                blocks: self.flow(&element.children),
            }),
            "code" | "preformat" => out.push(code(element)),
            "ref-list" => self.ref_list(element, out),
            "ref" => out.push(Block::Reference(self.reference(element))),
            _ => out.extend(self.flow(&element.children)),
        }
    }

    fn paragraph(
        &self,
        element: &Element,
    ) -> Option<Block> {
        let id = element.attr("id").map(str::to_owned);
        if self.in_norm_refs && is_citation_paragraph(element, id.as_deref()) {
            return Some(Block::Reference(self.citation_paragraph(element)));
        }
        let content = self.inlines(&element.children);
        if content.is_empty() && id.is_none() {
            return None;
        }
        Some(Block::Paragraph {
            id,
            indent: indent_of(element),
            content,
        })
    }

    fn list(
        &self,
        element: &Element,
    ) -> Block {
        Block::List {
            ordered: element.attr("list-type") == Some("order"),
            items: element
                .children_named("list-item")
                .map(|item| self.flow(&item.children))
                .collect(),
        }
    }

    fn table_wrap(
        &self,
        element: &Element,
    ) -> Table {
        let (caption_label, caption) = self.caption(element);
        let label = element
            .child("label")
            .map(|l| l.text().trim().to_owned())
            .or(caption_label);
        let tables: Vec<&Element> = element.children_named("table").collect();
        self.table(&tables, element.attr("id").map(str::to_owned), label, caption)
    }

    fn table(
        &self,
        tables: &[&Element],
        id: Option<String>,
        label: Option<String>,
        caption: Vec<Inline>,
    ) -> Table {
        let mut head = Vec::new();
        let mut body = Vec::new();
        for (i, table) in tables.iter().enumerate() {
            for part in table.elements() {
                match part.name.as_str() {
                    "thead" if i == 0 => head.extend(part.children_named("tr").map(|tr| self.row(tr))),
                    "thead" => body.extend(part.children_named("tr").map(|tr| self.row(tr))),
                    "tbody" | "tfoot" => body.extend(part.children_named("tr").map(|tr| self.row(tr))),
                    "tr" => body.push(self.row(part)),
                    _ => {}
                }
            }
        }
        Table {
            id,
            label,
            caption,
            head,
            body,
        }
    }

    fn row(
        &self,
        tr: &Element,
    ) -> Row {
        let cells = tr
            .elements()
            .filter(|cell| matches!(cell.name.as_str(), "td" | "th"))
            .map(|cell| Cell {
                header: cell.name == "th",
                colspan: cell
                    .attr("colspan")
                    .and_then(|span| span.parse().ok())
                    .unwrap_or(1)
                    .max(1),
                blocks: self.flow(&cell.children),
            })
            .collect();
        Row { cells }
    }

    fn figure(
        &self,
        element: &Element,
    ) -> Block {
        let (caption_label, caption) = self.caption(element);
        Block::Figure {
            id: element.attr("id").map(str::to_owned),
            label: element
                .child("label")
                .map(|l| l.text().trim().to_owned())
                .or(caption_label),
            caption,
            image: element
                .child("graphic")
                .and_then(|g| g.attr("xlink:href"))
                .map(str::to_owned),
        }
    }

    fn caption(
        &self,
        element: &Element,
    ) -> (Option<String>, Vec<Inline>) {
        let Some(caption) = element.child("caption") else {
            return (None, Vec::new());
        };
        let inlines = match caption.child("p") {
            Some(p) => self.inlines(&p.children),
            None => self.inlines(&caption.children),
        };
        split_caption_label(inlines)
    }

    fn ref_list(
        &self,
        element: &Element,
        out: &mut Vec<Block>,
    ) {
        for child in element.elements() {
            match child.name.as_str() {
                "title" => out.push(Block::Paragraph {
                    id: None,
                    indent: 0,
                    content: vec![Inline::Bold(self.inlines(&child.children))],
                }),
                "ref" => out.push(Block::Reference(self.reference(child))),
                _ => self.push_block(child, out),
            }
        }
    }

    fn reference(
        &self,
        element: &Element,
    ) -> Reference {
        let citation_nodes = element
            .child("mixed-citation")
            .map_or(element.children.as_slice(), |c| &c.children);
        let citation = citation_of(element);
        Reference {
            id: element.attr("id").map(str::to_owned),
            label: element.child("label").map(|l| l.text().trim().to_owned()),
            citation: self.inlines(citation_nodes),
            spec: citation.spec,
            url: citation.url,
        }
    }

    fn citation_paragraph(
        &self,
        element: &Element,
    ) -> Reference {
        let citation = citation_of(element);
        Reference {
            id: element.attr("id").map(str::to_owned),
            label: None,
            citation: self.inlines(&element.children),
            spec: citation.spec,
            url: citation.url,
        }
    }
}

fn is_citation_paragraph(
    element: &Element,
    id: Option<&str>,
) -> bool {
    id.is_some_and(|id| id.starts_with("ref_")) || spec_of_citation(&element.text()).is_some()
}

fn indent_of(element: &Element) -> u8 {
    element
        .attr("content-type")
        .and_then(|t| t.strip_prefix("indent-"))
        .and_then(|n| n.parse().ok())
        .unwrap_or(0)
}

fn code(element: &Element) -> Block {
    let text = element.text();
    let text = text.strip_suffix('\n').unwrap_or(&text);
    Block::Code {
        text: text.strip_suffix('\r').unwrap_or(text).to_owned(),
    }
}

/// Splits `Table 108 - AdditionalParametersType` into the label and the caption proper.
fn split_caption_label(mut inlines: Vec<Inline>) -> (Option<String>, Vec<Inline>) {
    let Some(Inline::Text(first)) = inlines.first() else {
        return (None, inlines);
    };
    if !first.starts_with("Table ") && !first.starts_with("Figure ") {
        return (None, inlines);
    }
    let separator = CAPTION_SEPARATORS
        .iter()
        .filter_map(|sep| first.find(sep).map(|at| (at, sep.len())))
        .min();
    let (label, rest) = match separator {
        Some((at, len)) => (first[..at].trim().to_owned(), first[at + len..].to_owned()),
        None if inlines.len() == 1 && first.split_whitespace().count() == 2 => (first.trim().to_owned(), String::new()),
        None => return (None, inlines),
    };
    if label.split_whitespace().count() != 2 {
        return (None, inlines);
    }
    if rest.is_empty() {
        inlines.remove(0);
    } else {
        inlines[0] = Inline::Text(rest);
    }
    (Some(label), inlines)
}
