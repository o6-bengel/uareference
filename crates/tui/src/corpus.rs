use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Context;
use rayon::prelude::*;
use uareference_model::text::web_url;
use uareference_model::{
    Block,
    Catalog,
    Inline,
    Link,
    Section,
    Spec,
    Store,
};

pub type SpecIdx = u16;

/// A place in a specification: a section and a slot, where slot 0 is the heading and slot `n` is block `n - 1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Loc {
    pub spec: SpecIdx,
    pub section: u32,
    pub slot: u32,
}

impl Loc {
    pub fn section(
        spec: SpecIdx,
        section: u32,
    ) -> Self {
        Self { spec, section, slot: 0 }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Loc(Loc),
    Spec(SpecIdx),
    Url(String),
    Missing(String),
}

pub struct SpecEntry {
    pub spec: Spec,
    pub anchors: HashMap<String, (u32, u32)>,
    pub labels: HashMap<String, u32>,
}

impl SpecEntry {
    pub fn new(spec: Spec) -> Self {
        let mut anchors = HashMap::new();
        let mut labels = HashMap::new();
        for (s, section) in spec.sections.iter().enumerate() {
            let s = s as u32;
            anchors.insert(section.id.clone(), (s, 0));
            if let Some(label) = &section.label {
                labels.entry(label.clone()).or_insert(s);
            }
            for (b, block) in section.blocks.iter().enumerate() {
                for_each_block_id(block, &mut |id| {
                    anchors.entry(id.to_owned()).or_insert((s, b as u32 + 1));
                });
            }
        }
        Self { spec, anchors, labels }
    }

    pub fn section(
        &self,
        section: u32,
    ) -> &Section {
        &self.spec.sections[section as usize]
    }

    /// Ancestors of a section from the top level down, including the section itself.
    pub fn breadcrumb(
        &self,
        section: u32,
    ) -> Vec<u32> {
        let mut chain = vec![section];
        let mut current = self.section(section).parent;
        while let Some(parent) = current {
            chain.push(parent);
            current = self.section(parent).parent;
        }
        chain.reverse();
        chain
    }

    pub fn heading(
        &self,
        section: u32,
    ) -> String {
        let s = self.section(section);
        match &s.label {
            Some(label) => format!("{label} {}", s.title),
            None => s.title.clone(),
        }
    }

    pub fn url(
        &self,
        section: Option<u32>,
    ) -> String {
        let label = section.and_then(|s| self.section(s).label.as_deref());
        web_url(&self.spec.id, label)
    }
}

fn for_each_block_id(
    block: &Block,
    f: &mut impl FnMut(&str),
) {
    match block {
        Block::Paragraph { id: Some(id), .. } => f(id),
        Block::Table(table) => {
            if let Some(id) = &table.id {
                f(id);
            }
            for cell in table.head.iter().chain(&table.body).flat_map(|r| &r.cells) {
                cell.blocks.iter().for_each(|b| for_each_block_id(b, f));
            }
        }
        Block::Figure { id: Some(id), .. } => f(id),
        Block::Reference(reference) => {
            if let Some(id) = &reference.id {
                f(id);
            }
        }
        Block::List { items, .. } => items.iter().flatten().for_each(|b| for_each_block_id(b, f)),
        Block::Note { blocks, .. } => blocks.iter().for_each(|b| for_each_block_id(b, f)),
        _ => {}
    }
}

/// Every link and every outermost italic of a block, in one canonical order shared by rendering and indexing.
pub fn for_each_ref<'a>(
    block: &'a Block,
    f: &mut impl FnMut(&'a Inline),
) {
    match block {
        Block::Paragraph { content, .. } => refs_in(content, f),
        Block::List { items, .. } => items.iter().flatten().for_each(|b| for_each_ref(b, f)),
        Block::Table(table) => {
            refs_in(&table.caption, f);
            for cell in table.head.iter().chain(&table.body).flat_map(|r| &r.cells) {
                cell.blocks.iter().for_each(|b| for_each_ref(b, f));
            }
        }
        Block::Figure { caption, .. } => refs_in(caption, f),
        Block::Note { blocks, .. } => blocks.iter().for_each(|b| for_each_ref(b, f)),
        Block::Code { .. } => {}
        Block::Reference(reference) => refs_in(&reference.citation, f),
    }
}

fn refs_in<'a>(
    inlines: &'a [Inline],
    f: &mut impl FnMut(&'a Inline),
) {
    for inline in inlines {
        match inline {
            Inline::Link { .. } | Inline::Italic(_) => f(inline),
            Inline::Bold(c)
            | Inline::Underline(c)
            | Inline::Strike(c)
            | Inline::Sup(c)
            | Inline::Sub(c)
            | Inline::Mono(c) => refs_in(c, f),
            Inline::Text(_) | Inline::Image { .. } | Inline::Break => {}
        }
    }
}

/// Ordinal of each ref inline of a block, keyed by address, so a renderer can tag spans in any order.
pub fn ref_ordinals(block: &Block) -> HashMap<usize, u32> {
    let mut map = HashMap::new();
    let mut n = 0;
    for_each_ref(block, &mut |inline| {
        map.insert(std::ptr::from_ref(inline) as usize, n);
        n += 1;
    });
    map
}

pub struct Library {
    pub store: Store,
    pub catalog: Catalog,
    pub ids: Vec<String>,
    pub entries: Vec<Option<Arc<SpecEntry>>>,
    by_id: HashMap<String, SpecIdx>,
}

impl Library {
    pub fn open(store: Store) -> anyhow::Result<Self> {
        let catalog = store.load_catalog().context("loading catalog")?;
        let stored = store.spec_ids().context("listing specifications")?;
        let mut ids: Vec<String> = catalog
            .entries
            .iter()
            .map(|e| e.id.clone())
            .filter(|id| stored.contains(id))
            .collect();
        ids.extend(stored.into_iter().filter(|id| catalog.get(id).is_none()));
        let by_id = ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id.clone(), i as SpecIdx))
            .collect();
        let entries = vec![None; ids.len()];
        Ok(Self {
            store,
            catalog,
            ids,
            entries,
            by_id,
        })
    }

    pub fn idx(
        &self,
        id: &str,
    ) -> Option<SpecIdx> {
        self.by_id.get(id).copied()
    }

    pub fn entry(
        &self,
        idx: SpecIdx,
    ) -> Option<&Arc<SpecEntry>> {
        self.entries.get(idx as usize)?.as_ref()
    }

    pub fn load(
        &mut self,
        idx: SpecIdx,
    ) -> anyhow::Result<Arc<SpecEntry>> {
        if let Some(entry) = self.entry(idx) {
            return Ok(entry.clone());
        }
        let entry = load_entry(&self.store, &self.ids[idx as usize])?;
        self.entries[idx as usize] = Some(entry.clone());
        Ok(entry)
    }

    pub fn title(
        &self,
        idx: SpecIdx,
    ) -> &str {
        let id = &self.ids[idx as usize];
        self.catalog
            .get(id)
            .map_or(id.as_str(), |e| e.title.as_str())
    }

    pub fn resolve_link(
        &self,
        ctx: SpecIdx,
        link: &Link,
    ) -> Target {
        match link {
            Link::Anchor { id } => match self.entry(ctx).and_then(|e| e.anchors.get(id)) {
                Some(&(section, slot)) => Target::Loc(Loc {
                    spec: ctx,
                    section,
                    slot,
                }),
                None => Target::Missing(format!("no element `{id}` in this document")),
            },
            Link::Spec { spec, section } => self.spec_target(spec, section.as_deref()),
            Link::Url { url } => Target::Url(url.clone()),
        }
    }

    pub fn spec_target(
        &self,
        spec: &str,
        label: Option<&str>,
    ) -> Target {
        let Some(idx) = self.idx(spec) else {
            return match self.catalog.get(spec) {
                Some(_) => Target::Url(web_url(spec, label)),
                None => Target::Missing(format!("{spec} is not a document of the online reference")),
            };
        };
        let section = label.and_then(|l| self.entry(idx)?.labels.get(l).copied());
        match section {
            Some(section) => Target::Loc(Loc::section(idx, section)),
            None => Target::Spec(idx),
        }
    }
}

pub fn load_entry(
    store: &Store,
    id: &str,
) -> anyhow::Result<Arc<SpecEntry>> {
    let spec = store
        .load_spec(id)
        .with_context(|| format!("loading {id}"))?;
    Ok(Arc::new(SpecEntry::new(spec)))
}

pub fn load_all(
    store: &Store,
    ids: &[String],
) -> Vec<Option<Arc<SpecEntry>>> {
    ids.par_iter()
        .map(|id| load_entry(store, id).ok())
        .collect()
}
