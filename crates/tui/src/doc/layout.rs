use std::collections::HashMap;

use uareference_model::text::plain;
use uareference_model::{
    Block,
    Inline,
    NodeRole,
    Reference,
};
use unicode_width::UnicodeWidthStr;

use super::wrap::{
    Frag,
    Word,
    words,
    wrap,
};
use super::{
    BOLD,
    Doc,
    ITALIC,
    Line,
    LinkKind,
    LinkRef,
    MONO,
    Origin,
    Role,
    STRIKE,
    SUB,
    SUP,
    Span,
    Sty,
    UNDERLINE,
    table,
};
use crate::corpus::{
    Library,
    SpecEntry,
    SpecIdx,
    ref_ordinals,
};
use crate::index::{
    Index,
    short_node_id,
};

pub const MARGIN: usize = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Opts {
    pub width: usize,
    pub textwidth: usize,
}

pub fn layout_spec(
    entry: &SpecEntry,
    idx: SpecIdx,
    index: Option<&Index>,
    opts: Opts,
) -> Doc {
    let mut builder = Builder::new(entry, idx, index, opts);
    for s in 0..entry.spec.sections.len() {
        builder.section(s as u32);
    }
    builder.finish()
}

pub fn layout_section(
    entry: &SpecEntry,
    idx: SpecIdx,
    index: Option<&Index>,
    opts: Opts,
    section: u32,
) -> Doc {
    let mut builder = Builder::new(entry, idx, index, opts);
    builder.section(section);
    builder.finish()
}

pub fn layout_home(
    lib: &Library,
    width: usize,
) -> Doc {
    let mut doc = Doc::default();
    let mut push = |spans: Vec<Span>, row: &mut u32| {
        let margin_link = spans.first().and_then(|s| s.link);
        let mut line = vec![Span::new(" ", Sty::default(), margin_link)];
        line.extend(spans);
        doc.lines.push(Line {
            spans: line,
            origin: Origin {
                section: 0,
                slot: 0,
                row: *row,
            },
        });
        *row += 1;
    };
    let mut row = 0;
    let title = format!("OPC UA Online Reference — {} documents", lib.ids.len());
    push(vec![Span::new(title, Sty::role(Role::Title).with(BOLD), None)], &mut row);
    push(Vec::new(), &mut row);
    if lib.ids.is_empty() {
        let hint = "Nothing synced yet. Run `uareference sync` in a shell, or `:sync` here.";
        push(vec![Span::new(hint, Sty::role(Role::Muted), None)], &mut row);
    }
    let id_width = lib.ids.iter().map(|id| id.width()).max().unwrap_or(0);
    for (i, id) in lib.ids.iter().enumerate() {
        let entry = lib.catalog.get(id);
        let version = entry.map_or("", |e| e.version.as_str());
        let title = lib.title(i as SpecIdx);
        let link = doc.links.len() as u32;
        doc.links.push(LinkRef {
            kind: LinkKind::Spec(i as SpecIdx),
            section: 0,
            slot: 0,
            nth: u32::MAX,
        });
        let text = format!("{id:<id_width$}  {version:<9} ");
        let rest = width.saturating_sub(text.width() + 2);
        let title: String = title.chars().take(rest).collect();
        push(
            vec![
                Span::new(text, Sty::role(Role::Header), Some(link)),
                Span::new(title, Sty::default(), Some(link)),
            ],
            &mut row,
        );
    }
    doc
}

pub struct Builder<'a> {
    doc: Doc,
    idx: SpecIdx,
    entry: &'a SpecEntry,
    index: Option<&'a Index>,
    opts: Opts,
    section: u32,
    slot: u32,
    row: u32,
    ordinals: HashMap<usize, u32>,
}

impl<'a> Builder<'a> {
    fn new(
        entry: &'a SpecEntry,
        idx: SpecIdx,
        index: Option<&'a Index>,
        opts: Opts,
    ) -> Self {
        let doc = Doc {
            headings: vec![None; entry.spec.sections.len()],
            ..Doc::default()
        };
        Self {
            doc,
            idx,
            entry,
            index,
            opts,
            section: 0,
            slot: 0,
            row: 0,
            ordinals: HashMap::new(),
        }
    }

    fn finish(self) -> Doc {
        self.doc
    }

    fn full_width(&self) -> usize {
        self.opts.width.saturating_sub(2 * MARGIN).max(20)
    }

    fn text_width(&self) -> usize {
        self.full_width().min(self.opts.textwidth.max(20))
    }

    fn push(
        &mut self,
        spans: Vec<Span>,
    ) {
        let mut line = Vec::with_capacity(spans.len() + 1);
        line.push(Span::new(" ".repeat(MARGIN), Sty::default(), None));
        line.extend(spans);
        let origin = Origin {
            section: self.section,
            slot: self.slot,
            row: self.row,
        };
        self.doc.lines.push(Line { spans: line, origin });
        self.row += 1;
    }

    fn blank(&mut self) {
        self.push(Vec::new());
    }

    fn section(
        &mut self,
        s: u32,
    ) {
        let entry = self.entry;
        let section = entry.section(s);
        self.section = s;
        self.slot = 0;
        self.row = 0;
        self.doc.headings[s as usize] = Some(self.doc.lines.len() as u32);
        self.heading(s);
        for (b, block) in section.blocks.iter().enumerate() {
            self.top_block(b, block);
        }
    }

    fn heading(
        &mut self,
        s: u32,
    ) {
        let section = self.entry.section(s);
        let sty = Sty::role(Role::Heading(section.depth)).with(BOLD);
        let mut heading = Vec::new();
        if let Some(label) = &section.label {
            heading.push(Word::plain(label, sty));
        }
        heading.extend(words(&[Frag::Text {
            text: section.title.clone(),
            sty,
            link: None,
        }]));
        let lines = wrap(&heading, self.text_width());
        let underline = lines
            .iter()
            .map(|l| l.iter().map(|s| s.width as usize).sum::<usize>())
            .max()
            .unwrap_or(0);
        for line in lines {
            self.push(line);
        }
        if section.depth == 0 {
            self.push(vec![Span::new("━".repeat(underline), Sty::role(Role::Heading(0)), None)]);
        }
        if let Some(meta) = node_summary(section) {
            let lines = wrap(
                &words(&[Frag::Text {
                    text: meta,
                    sty: Sty::role(Role::Meta),
                    link: None,
                }]),
                self.text_width(),
            );
            for line in lines {
                self.push(line);
            }
        }
        self.blank();
    }

    fn top_block(
        &mut self,
        b: usize,
        block: &Block,
    ) {
        self.slot = b as u32 + 1;
        self.row = 0;
        self.ordinals = ref_ordinals(block);
        let width = match block {
            Block::Table(_) => self.full_width(),
            _ => self.text_width(),
        };
        for line in self.block_lines(block, width) {
            self.push(line);
        }
        self.blank();
    }

    pub(super) fn block_lines(
        &mut self,
        block: &Block,
        width: usize,
    ) -> Vec<Vec<Span>> {
        match block {
            Block::Paragraph { indent, content, .. } => {
                let indent = 2 * *indent as usize;
                let lines = wrap(&self.words(content, Sty::default()), width.saturating_sub(indent));
                prefixed(lines, &" ".repeat(indent), &" ".repeat(indent), Sty::default())
            }
            Block::List { ordered, items } => self.list(*ordered, items, width),
            Block::Table(t) => table::layout(self, t, width),
            Block::Figure { id, label, caption, .. } => self.figure(id.as_deref(), label.as_deref(), caption, width),
            Block::Note { label, blocks } => self.note(label.as_deref(), blocks, width),
            Block::Code { text } => code_lines(text, width),
            Block::Reference(reference) => self.reference(reference, width),
        }
    }

    fn list(
        &mut self,
        ordered: bool,
        items: &[Vec<Block>],
        width: usize,
    ) -> Vec<Vec<Span>> {
        let mut out = Vec::new();
        for (i, item) in items.iter().enumerate() {
            let marker = if ordered {
                format!("{}. ", i + 1)
            } else {
                "• ".to_owned()
            };
            let pad = " ".repeat(marker.width());
            for (j, block) in item.iter().enumerate() {
                let lines = self.block_lines(block, width.saturating_sub(pad.len()));
                let first = if j == 0 { marker.as_str() } else { pad.as_str() };
                out.extend(prefixed(lines, first, &pad, Sty::role(Role::Bullet)));
            }
        }
        out
    }

    fn note(
        &mut self,
        label: Option<&str>,
        blocks: &[Block],
        width: usize,
    ) -> Vec<Vec<Span>> {
        let mut out = Vec::new();
        if let Some(label) = label {
            out.push(vec![Span::new(label, Sty::role(Role::Note).with(BOLD), None)]);
        }
        for block in blocks {
            out.extend(self.block_lines(block, width.saturating_sub(2)));
        }
        prefixed(out, "│ ", "│ ", Sty::role(Role::Note))
    }

    fn figure(
        &mut self,
        id: Option<&str>,
        label: Option<&str>,
        caption: &[Inline],
        width: usize,
    ) -> Vec<Vec<Span>> {
        let section = self.entry.section(self.section);
        let mut url = self.entry.url(Some(self.section));
        if let Some(id) = id {
            url = format!("{url}#{id}");
        }
        let link = self.add_link(None, LinkKind::Figure { url });
        let sty = Sty::role(Role::Caption);
        let mut frags = vec![Frag::Text {
            text: "▣ ".to_owned(),
            sty,
            link: Some(link),
        }];
        let label = label.map_or_else(|| format!("Figure in {}", section.title), str::to_owned);
        frags.push(Frag::Text {
            text: label,
            sty: sty.with(BOLD),
            link: Some(link),
        });
        if !caption.is_empty() {
            frags.push(Frag::Text {
                text: " – ".to_owned(),
                sty,
                link: Some(link),
            });
            self.frags(caption, sty, Some(link), &mut frags);
        }
        frags.push(Frag::Text {
            text: "  (gx opens it in the browser)".to_owned(),
            sty: Sty::role(Role::Muted),
            link: None,
        });
        wrap(&words(&frags), width)
    }

    fn reference(
        &mut self,
        reference: &Reference,
        width: usize,
    ) -> Vec<Vec<Span>> {
        let target = match (&reference.spec, &reference.url) {
            (Some(spec), _) => Some(LinkKind::Link(uareference_model::Link::Spec {
                spec: spec.clone(),
                section: None,
            })),
            (None, Some(url)) => Some(LinkKind::Link(uareference_model::Link::Url { url: url.clone() })),
            (None, None) => None,
        };
        let link = target.map(|kind| self.add_link(None, kind));
        let mut frags = Vec::new();
        if let Some(label) = &reference.label {
            frags.push(Frag::Text {
                text: format!("[{label}] "),
                sty: Sty::role(Role::Muted),
                link: None,
            });
        }
        self.frags(&reference.citation, Sty::default(), link, &mut frags);
        if let (Some(url), Some(_)) = (&reference.url, &reference.spec) {
            frags.push(Frag::Text {
                text: format!(" <{url}>"),
                sty: Sty::role(Role::Muted),
                link: None,
            });
        }
        wrap(&words(&frags), width)
    }

    pub(super) fn words(
        &mut self,
        inlines: &[Inline],
        sty: Sty,
    ) -> Vec<Word> {
        let mut frags = Vec::new();
        self.frags(inlines, sty, None, &mut frags);
        words(&frags)
    }

    pub(super) fn frags(
        &mut self,
        inlines: &[Inline],
        sty: Sty,
        link: Option<u32>,
        out: &mut Vec<Frag>,
    ) {
        for inline in inlines {
            match inline {
                Inline::Text(text) => out.push(Frag::Text {
                    text: text.clone(),
                    sty,
                    link,
                }),
                Inline::Break => out.push(Frag::Break),
                Inline::Image { .. } => out.push(Frag::Text {
                    text: "[image]".to_owned(),
                    sty: Sty::role(Role::Muted),
                    link,
                }),
                Inline::Bold(c) => self.frags(c, sty.with(BOLD), link, out),
                Inline::Underline(c) => self.frags(c, sty.with(UNDERLINE), link, out),
                Inline::Strike(c) => self.frags(c, sty.with(STRIKE), link, out),
                Inline::Sup(c) => self.frags(c, sty.with(SUP), link, out),
                Inline::Sub(c) => self.frags(c, sty.with(SUB), link, out),
                Inline::Mono(c) => self.frags(c, sty.with(MONO), link, out),
                Inline::Italic(c) => {
                    let link = link.or_else(|| Some(self.term_link(inline, c)));
                    self.frags(c, sty.with(ITALIC), link, out);
                }
                Inline::Link { target, content } => {
                    let id = self.add_link(Some(inline), LinkKind::Link(target.clone()));
                    self.frags(content, sty, Some(id), out);
                }
            }
        }
    }

    fn term_link(
        &mut self,
        inline: &Inline,
        children: &[Inline],
    ) -> u32 {
        let text = plain(children);
        let symbol = self.index.and_then(|index| index.resolve(&text, self.idx));
        self.add_link(Some(inline), LinkKind::Term { text, symbol })
    }

    fn add_link(
        &mut self,
        inline: Option<&Inline>,
        kind: LinkKind,
    ) -> u32 {
        let nth = inline
            .and_then(|i| {
                self.ordinals
                    .get(&(std::ptr::from_ref(i) as usize))
                    .copied()
            })
            .unwrap_or(u32::MAX);
        self.doc.links.push(LinkRef {
            kind,
            section: self.section,
            slot: self.slot,
            nth,
        });
        self.doc.links.len() as u32 - 1
    }
}

fn node_summary(section: &uareference_model::Section) -> Option<String> {
    let parts: Vec<String> = section
        .nodes
        .iter()
        .filter(|n| matches!(n.role, NodeRole::Type | NodeRole::Object))
        .map(|n| {
            let class = n.node_class.as_deref().unwrap_or("Node");
            let id = n.node_id.as_deref().map(short_node_id).unwrap_or_default();
            let mut text = format!("{class} {id}");
            if !n.conformance_units.is_empty() {
                text.push_str(&format!("  CU: {}", n.conformance_units.join(", ")));
            }
            text
        })
        .collect();
    (!parts.is_empty()).then(|| format!("⟨{}⟩", parts.join(" · ")))
}

pub(super) fn code_lines(
    text: &str,
    width: usize,
) -> Vec<Vec<Span>> {
    let sty = Sty::role(Role::Code);
    let mut out = Vec::new();
    for raw in text.lines() {
        let line = raw.replace('\t', "    ");
        let mut rest = line.as_str();
        loop {
            let cut = rest.char_indices().scan(0, |w, (i, c)| {
                *w += unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
                Some((i, *w))
            });
            let split = cut
                .take_while(|&(_, w)| w <= width.max(1))
                .last()
                .map_or(0, |(i, _)| i);
            let end = rest[split..]
                .chars()
                .next()
                .map_or(rest.len(), |c| split + c.len_utf8());
            let (head, tail) = rest.split_at(end.min(rest.len()));
            out.push(vec![Span::new(head, sty, None)]);
            if tail.is_empty() {
                break;
            }
            rest = tail;
        }
    }
    out
}

pub(super) fn prefixed(
    lines: Vec<Vec<Span>>,
    first: &str,
    rest: &str,
    sty: Sty,
) -> Vec<Vec<Span>> {
    lines
        .into_iter()
        .enumerate()
        .map(|(i, line)| {
            let prefix = if i == 0 { first } else { rest };
            if prefix.is_empty() {
                return line;
            }
            let mut out = Vec::with_capacity(line.len() + 1);
            out.push(Span::new(prefix, sty, None));
            out.extend(line);
            out
        })
        .collect()
}
