use uareference_model::{
    Block,
    Inline,
    Section,
    SectionKind,
};

use super::Builder;
use super::inlines::{
    collapse_whitespace,
    is_block,
};
use super::meta::node_metas;
use crate::dom::Element;

/// Numbering state of the `term-sec` children of one section.
#[derive(Default)]
struct Terms {
    ordinal: usize,
    last: Option<usize>,
}

impl Builder<'_> {
    pub fn front(
        &mut self,
        front: &Element,
    ) {
        for child in front.elements() {
            match child.name.as_str() {
                "foreword" => self.section(child, 0, None, SectionKind::Foreword, "Foreword"),
                "intro" => self.section(child, 0, None, SectionKind::Intro, "Introduction"),
                "sec" => self.section(child, 0, None, kind_of(child), ""),
                _ => {}
            }
        }
    }

    pub fn body(
        &mut self,
        body: &Element,
    ) {
        for child in body.children_named("sec") {
            self.section(child, 0, None, kind_of(child), "");
        }
    }

    pub fn back(
        &mut self,
        back: &Element,
    ) {
        for child in back.elements() {
            match child.name.as_str() {
                "ref-list" => self.section(child, 0, None, SectionKind::Bibliography, "Bibliography"),
                "sec" => self.section(child, 0, None, kind_of(child), ""),
                _ => {}
            }
        }
    }

    fn section(
        &mut self,
        element: &Element,
        depth: u8,
        parent: Option<u32>,
        kind: SectionKind,
        default_title: &str,
    ) {
        let index = self.sections.len();
        let title = element
            .child("title")
            .map(heading_text)
            .filter(|t| !t.is_empty());
        let id = self.section_id(element);
        self.sections.push(Section {
            id,
            label: element
                .child("label")
                .map(heading_text)
                .filter(|l| !l.is_empty()),
            title: title.unwrap_or_else(|| default_title.to_owned()),
            depth,
            parent,
            kind,
            nodes: element
                .child("sec-meta")
                .map(node_metas)
                .unwrap_or_default(),
            blocks: Vec::new(),
        });
        let outer_norm_refs = self.in_norm_refs;
        self.in_norm_refs |= kind == SectionKind::NormativeReferences;
        let blocks = self.section_content(element, index, depth);
        self.sections[index].blocks = blocks;
        self.in_norm_refs = outer_norm_refs;
    }

    fn section_content(
        &mut self,
        element: &Element,
        index: usize,
        depth: u8,
    ) -> Vec<Block> {
        let mut blocks = Vec::new();
        let mut terms = Terms::default();
        let mut run_start = 0;
        for (i, node) in element.children.iter().enumerate() {
            let Some(child) = node.as_element().filter(|c| is_structural(c)) else {
                continue;
            };
            self.push_run(&element.children[run_start..i], &mut blocks);
            run_start = i + 1;
            match child.name.as_str() {
                "label" | "title" | "sec-meta" => {}
                "sec" => self.section(child, depth + 1, Some(index_u32(index)), kind_of(child), ""),
                "term-sec" => self.term(child, depth + 1, index, &mut terms, &mut blocks),
                _ => self.push_block(child, &mut blocks),
            }
        }
        self.push_run(&element.children[run_start..], &mut blocks);
        blocks
    }

    fn term(
        &mut self,
        element: &Element,
        depth: u8,
        parent: usize,
        terms: &mut Terms,
        parent_blocks: &mut Vec<Block>,
    ) {
        let (number, name) = term_heading(element);
        let blocks = self.term_blocks(element);
        if name.is_none() && blocks.is_empty() {
            return;
        }
        if number.is_none() && name.is_none() {
            match terms.last {
                Some(last) => self.sections[last].blocks.extend(blocks),
                None => parent_blocks.extend(blocks),
            }
            return;
        }
        terms.ordinal += 1;
        let parent_label = self.sections[parent].label.as_deref();
        let label = number.or_else(|| parent_label.map(|p| format!("{p}.{}", terms.ordinal)));
        let id = self.section_id(element);
        terms.last = Some(self.sections.len());
        self.sections.push(Section {
            id,
            label,
            title: name.unwrap_or_default(),
            depth,
            parent: Some(index_u32(parent)),
            kind: SectionKind::Term,
            nodes: Vec::new(),
            blocks,
        });
    }

    fn term_blocks(
        &self,
        element: &Element,
    ) -> Vec<Block> {
        let mut blocks = Vec::new();
        for child in element.elements() {
            match child.name.as_str() {
                "label" | "tbx:term" => {}
                "source" => blocks.push(self.source(child)),
                _ => self.push_block(child, &mut blocks),
            }
        }
        blocks
    }

    fn source(
        &self,
        element: &Element,
    ) -> Block {
        let mut content = self.inlines(&element.children);
        let text = element.text();
        let text = text.trim_start().trim_start_matches('[');
        if !text.starts_with("SOURCE") {
            content.insert(0, Inline::Text("SOURCE: ".into()));
        }
        Block::Paragraph {
            id: None,
            indent: 0,
            content,
        }
    }

    fn section_id(
        &mut self,
        element: &Element,
    ) -> String {
        match element.attr("id").filter(|id| !id.is_empty()) {
            Some(id) => id.to_owned(),
            None => {
                self.next_auto_id += 1;
                format!("auto_{}", self.next_auto_id)
            }
        }
    }
}

fn is_structural(element: &Element) -> bool {
    matches!(element.name.as_str(), "label" | "title" | "sec-meta" | "sec" | "term-sec") || is_block(element)
}

fn kind_of(element: &Element) -> SectionKind {
    match element.attr("sec-type") {
        Some("annex") => SectionKind::Annex,
        Some("terms") => SectionKind::Terms,
        Some("norm-refs") => SectionKind::NormativeReferences,
        Some("scope") => SectionKind::Scope,
        Some("intro") => SectionKind::Intro,
        Some("symbols") => SectionKind::Symbols,
        _ => SectionKind::Clause,
    }
}

fn heading_text(element: &Element) -> String {
    collapse_whitespace(&element.text()).trim().to_owned()
}

/// The explicit clause number and the term of a `term-sec`. Some sources put the term, or
/// `3.2.1 Asset Management`, in `label` instead of `tbx:term`.
fn term_heading(element: &Element) -> (Option<String>, Option<String>) {
    let terms: Vec<String> = element
        .children_named("tbx:term")
        .map(heading_text)
        .filter(|t| !t.is_empty())
        .collect();
    let (number, label_name) = match element
        .child("label")
        .map(heading_text)
        .filter(|l| !l.is_empty())
    {
        Some(label) => split_numbered(&label),
        None => (None, None),
    };
    let name = if terms.is_empty() {
        label_name
    } else {
        Some(terms.join("; "))
    };
    (number, name)
}

fn split_numbered(label: &str) -> (Option<String>, Option<String>) {
    let (head, tail) = label.split_once(' ').unwrap_or((label, ""));
    let tail = (!tail.trim().is_empty()).then(|| tail.trim().to_owned());
    if is_clause_number(head) {
        (Some(head.to_owned()), tail)
    } else {
        (None, Some(label.to_owned()))
    }
}

fn is_clause_number(text: &str) -> bool {
    let mut parts = text.trim_end_matches('.').split('.');
    let first = parts.next().unwrap_or_default();
    let first_ok = first.chars().all(|c| c.is_ascii_digit())
        || (first.len() == 1 && first.chars().all(|c| c.is_ascii_uppercase()));
    !first.is_empty() && first_ok && parts.all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

fn index_u32(index: usize) -> u32 {
    u32::try_from(index).unwrap_or(u32::MAX)
}
