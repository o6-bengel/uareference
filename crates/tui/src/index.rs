use std::collections::HashMap;

use rayon::prelude::*;
use uareference_model::text::plain;
use uareference_model::{
    Block,
    Inline,
    NodeRole,
    SectionKind,
};

use crate::corpus::{
    Library,
    Loc,
    SpecEntry,
    SpecIdx,
    Target,
    for_each_ref,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SymKind {
    Type,
    Term,
    Section,
    Instance,
    Table,
    Component,
}

impl SymKind {
    pub fn icon(self) -> &'static str {
        match self {
            SymKind::Type => "T",
            SymKind::Term => "D",
            SymKind::Section => "§",
            SymKind::Instance => "O",
            SymKind::Table => "▦",
            SymKind::Component => "·",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub kind: SymKind,
    pub loc: Loc,
    pub node_class: Option<String>,
    pub node_id: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct Occurrence {
    pub loc: Loc,
    pub nth: u32,
}

/// Where a reference points: a section, or a whole document (`section == u32::MAX`).
pub type RefKey = (SpecIdx, u32);

pub const WHOLE_SPEC: u32 = u32::MAX;

pub struct Index {
    pub symbols: Vec<Symbol>,
    by_key: HashMap<String, Vec<u32>>,
    by_node_id: HashMap<String, Vec<u32>>,
    refs: HashMap<RefKey, Vec<Occurrence>>,
    core: Vec<bool>,
}

/// Lookup key of a name: alphanumerics only, lower-cased, so `Address Space`, `AddressSpace` and `addressspace.` agree.
pub fn key(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// The key of an italic phrase followed by its singular forms, most literal first.
pub fn term_keys(text: &str) -> Vec<String> {
    let k = key(text);
    if k.len() < 2 {
        return Vec::new();
    }
    let mut keys = vec![k.clone()];
    if let Some(stem) = k.strip_suffix("ies") {
        keys.push(format!("{stem}y"));
    }
    if let Some(stem) = k.strip_suffix("es") {
        keys.push(stem.to_owned());
    }
    if let Some(stem) = k.strip_suffix('s') {
        keys.push(stem.to_owned());
    }
    keys
}

pub fn short_node_id(node_id: &str) -> &str {
    node_id.rsplit_once(';').map_or(node_id, |(_, id)| id)
}

impl Index {
    pub fn build(lib: &Library) -> Self {
        let entries: Vec<(SpecIdx, &SpecEntry)> = lib
            .entries
            .iter()
            .enumerate()
            .filter_map(|(i, e)| Some((i as SpecIdx, e.as_deref()?)))
            .collect();
        let symbols: Vec<Symbol> = entries
            .iter()
            .flat_map(|&(idx, entry)| symbols_of(idx, entry))
            .collect();
        let mut index = Self {
            by_key: key_map(&symbols),
            by_node_id: node_id_map(&symbols),
            symbols,
            refs: HashMap::new(),
            core: lib.ids.iter().map(|id| is_core(id)).collect(),
        };
        let found: Vec<Vec<(RefKey, Occurrence)>> = entries
            .par_iter()
            .map(|&(idx, entry)| index.references_from(lib, idx, entry))
            .collect();
        for (key, occurrence) in found.into_iter().flatten() {
            index.refs.entry(key).or_default().push(occurrence);
        }
        index
    }

    /// Symbols an italic phrase may name, best first, one per location.
    pub fn candidates(
        &self,
        text: &str,
        ctx: SpecIdx,
    ) -> Vec<u32> {
        let Some(found) = term_keys(text).iter().find_map(|k| self.by_key.get(k)) else {
            return Vec::new();
        };
        let mut found = found.clone();
        found.sort_by_key(|&s| self.rank(&self.symbols[s as usize], ctx));
        let mut seen = Vec::new();
        found.retain(|&s| {
            let loc = self.symbols[s as usize].loc;
            let section = (loc.spec, loc.section);
            let fresh = !seen.contains(&section);
            seen.push(section);
            fresh
        });
        found
    }

    pub fn resolve(
        &self,
        text: &str,
        ctx: SpecIdx,
    ) -> Option<u32> {
        let k = term_keys(text);
        let found = k.iter().find_map(|k| self.by_key.get(k))?;
        found
            .iter()
            .copied()
            .min_by_key(|&s| self.rank(&self.symbols[s as usize], ctx))
    }

    pub fn symbol(
        &self,
        id: u32,
    ) -> &Symbol {
        &self.symbols[id as usize]
    }

    pub fn by_node_id(
        &self,
        node_id: &str,
    ) -> &[u32] {
        let node_id = node_id.trim();
        self.by_node_id
            .get(node_id)
            .or_else(|| self.by_node_id.get(short_node_id(node_id)))
            .map_or(&[], Vec::as_slice)
    }

    pub fn references(
        &self,
        key: RefKey,
    ) -> &[Occurrence] {
        self.refs.get(&key).map_or(&[], Vec::as_slice)
    }

    fn rank(
        &self,
        symbol: &Symbol,
        ctx: SpecIdx,
    ) -> (SymKind, u8, SpecIdx, u32) {
        let spec = symbol.loc.spec;
        let locality = if spec == ctx {
            0
        } else if self.core.get(spec as usize).copied().unwrap_or(false) {
            1
        } else {
            2
        };
        (symbol.kind, locality, spec, symbol.loc.section)
    }

    fn references_from(
        &self,
        lib: &Library,
        idx: SpecIdx,
        entry: &SpecEntry,
    ) -> Vec<(RefKey, Occurrence)> {
        let mut out = Vec::new();
        for (s, section) in entry.spec.sections.iter().enumerate() {
            for (b, block) in section.blocks.iter().enumerate() {
                let loc = Loc {
                    spec: idx,
                    section: s as u32,
                    slot: b as u32 + 1,
                };
                let mut nth = 0;
                for_each_ref(block, &mut |inline| {
                    if let Some(key) = self.ref_key(lib, idx, inline) {
                        out.push((key, Occurrence { loc, nth }));
                    }
                    nth += 1;
                });
            }
        }
        out
    }

    fn ref_key(
        &self,
        lib: &Library,
        ctx: SpecIdx,
        inline: &Inline,
    ) -> Option<RefKey> {
        match inline {
            Inline::Link { target, .. } => match lib.resolve_link(ctx, target) {
                Target::Loc(loc) => Some((loc.spec, loc.section)),
                Target::Spec(spec) => Some((spec, WHOLE_SPEC)),
                Target::Url(_) | Target::Missing(_) => None,
            },
            Inline::Italic(children) => {
                let loc = self.symbol(self.resolve(&plain(children), ctx)?).loc;
                Some((loc.spec, loc.section))
            }
            _ => None,
        }
    }
}

fn is_core(id: &str) -> bool {
    id.strip_prefix("OPC-10000-")
        .and_then(|n| n.parse::<u32>().ok())
        .is_some_and(|n| n < 100)
}

fn symbols_of(
    idx: SpecIdx,
    entry: &SpecEntry,
) -> Vec<Symbol> {
    let mut out = Vec::new();
    for (s, section) in entry.spec.sections.iter().enumerate() {
        let loc = Loc::section(idx, s as u32);
        let kind = if section.kind == SectionKind::Term {
            SymKind::Term
        } else {
            SymKind::Section
        };
        out.push(Symbol {
            name: section.title.clone(),
            kind,
            loc,
            node_class: None,
            node_id: None,
        });
        for node in &section.nodes {
            let (kind, name) = match (node.role, &node.browse_name, &node.path) {
                (NodeRole::Type, Some(name), _) => (SymKind::Type, name.clone()),
                (NodeRole::Object | NodeRole::Other, Some(name), _) => (SymKind::Instance, name.clone()),
                (_, _, Some(path)) => (SymKind::Component, path.clone()),
                (_, Some(name), None) => (SymKind::Instance, name.clone()),
                _ => continue,
            };
            out.push(Symbol {
                name,
                kind,
                loc,
                node_class: node.node_class.clone(),
                node_id: node.node_id.clone(),
            });
        }
        for (b, block) in section.blocks.iter().enumerate() {
            if let Block::Table(table) = block {
                let caption = plain(&table.caption);
                let caption = caption.trim();
                if !caption.is_empty() && !caption.contains(' ') {
                    let loc = Loc {
                        slot: b as u32 + 1,
                        ..loc
                    };
                    out.push(Symbol {
                        name: caption.to_owned(),
                        kind: SymKind::Table,
                        loc,
                        node_class: None,
                        node_id: None,
                    });
                }
            }
        }
    }
    out
}

fn key_map(symbols: &[Symbol]) -> HashMap<String, Vec<u32>> {
    let mut map: HashMap<String, Vec<u32>> = HashMap::new();
    for (i, symbol) in symbols.iter().enumerate() {
        if symbol.kind != SymKind::Component {
            map.entry(key(&symbol.name)).or_default().push(i as u32);
        }
    }
    map
}

fn node_id_map(symbols: &[Symbol]) -> HashMap<String, Vec<u32>> {
    let mut map: HashMap<String, Vec<u32>> = HashMap::new();
    for (i, symbol) in symbols.iter().enumerate() {
        if let Some(node_id) = &symbol.node_id {
            map.entry(node_id.clone()).or_default().push(i as u32);
            map.entry(short_node_id(node_id).to_owned())
                .or_default()
                .push(i as u32);
        }
    }
    map
}
