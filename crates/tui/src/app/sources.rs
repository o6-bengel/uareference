use std::sync::Arc;

use uareference_model::text::plain;

use super::keys::KEYMAP;
use super::picker::{
    Item,
    PickAction,
    Picker,
    Source,
};
use super::state::Place;
use super::{
    App,
    ViewKey,
};
use crate::corpus::{
    Loc,
    SpecEntry,
    SpecIdx,
    for_each_ref,
};
use crate::doc::Doc;
use crate::doc::layout::{
    Opts,
    layout_section,
};
use crate::index::{
    Occurrence,
    RefKey,
    SymKind,
    WHOLE_SPEC,
    short_node_id,
};
use crate::search::{
    slot_text,
    snippet,
};

const MAX_REFERENCES: usize = 5000;

impl App {
    fn open_picker(
        &mut self,
        picker: Picker,
    ) {
        self.hover = None;
        self.picker = Some(picker);
    }

    pub(super) fn pick_specs(&mut self) {
        let items = (0..self.lib.ids.len())
            .map(|i| {
                let id = self.lib.ids[i].clone();
                let version = self
                    .lib
                    .catalog
                    .get(&id)
                    .map(|e| e.version.clone())
                    .unwrap_or_default();
                let title = self.lib.title(i as SpecIdx).to_owned();
                Item {
                    haystack: format!("{id} {title}"),
                    detail: format!("{version:<9} {title}"),
                    label: id,
                    action: PickAction::Spec(i as SpecIdx),
                }
            })
            .collect();
        let current = self.current_spec().map_or(0, |s| s as usize);
        self.open_picker(Picker::new("Documents", items, Source::Fuzzy).select_item(current));
    }

    pub(super) fn pick_outline(&mut self) {
        let Some(spec) = self.current_spec() else {
            return self.pick_specs();
        };
        let Some(entry) = self.lib.entry(spec).cloned() else {
            return;
        };
        let items = entry
            .spec
            .sections
            .iter()
            .enumerate()
            .map(|(s, section)| {
                let heading = entry.heading(s as u32);
                let detail = section
                    .nodes
                    .iter()
                    .find_map(|n| n.node_class.clone())
                    .unwrap_or_default();
                Item {
                    label: format!("{}{heading}", "  ".repeat(section.depth as usize)),
                    detail,
                    haystack: heading,
                    action: PickAction::Loc(Loc::section(spec, s as u32), None),
                }
            })
            .collect();
        let current = self.current_section().unwrap_or(0) as usize;
        self.open_picker(
            Picker::new(format!("Outline — {}", entry.spec.id), items, Source::Fuzzy).select_item(current),
        );
    }

    pub(super) fn pick_symbols(
        &mut self,
        terms: bool,
    ) {
        let Some(index) = self.index.clone() else {
            return self.info("still indexing the documents…");
        };
        let wanted = |kind: SymKind| {
            if terms {
                kind == SymKind::Term
            } else {
                !matches!(kind, SymKind::Term | SymKind::Section)
            }
        };
        let items = index
            .symbols
            .iter()
            .enumerate()
            .filter(|(_, s)| wanted(s.kind))
            .map(|(i, s)| {
                let id = s.node_id.as_deref().map(short_node_id).unwrap_or_default();
                Item {
                    label: format!("{} {}", s.kind.icon(), s.name),
                    detail: format!("{} {id}  {}", s.node_class.as_deref().unwrap_or_default(), self.describe(s.loc))
                        .trim()
                        .to_owned(),
                    haystack: format!("{} {id} {}", s.name, s.node_id.as_deref().unwrap_or_default()),
                    action: PickAction::Loc(index.symbol(i as u32).loc, None),
                }
            })
            .collect();
        let title = if terms {
            "Terms"
        } else {
            "Symbols — T type · O object · · instance declaration · ▦ table"
        };
        self.open_picker(Picker::new(title, items, Source::Fuzzy));
    }

    pub(super) fn pick_headings(&mut self) {
        let Some(index) = self.index.clone() else {
            return self.info("still indexing the documents…");
        };
        let items = index
            .symbols
            .iter()
            .filter(|s| matches!(s.kind, SymKind::Section | SymKind::Term))
            .map(|s| {
                let spec = &self.lib.ids[s.loc.spec as usize];
                let heading = self
                    .lib
                    .entry(s.loc.spec)
                    .map(|e| e.heading(s.loc.section))
                    .unwrap_or_default();
                Item {
                    label: heading.clone(),
                    detail: spec.clone(),
                    haystack: format!("{heading} {spec}"),
                    action: PickAction::Loc(s.loc, None),
                }
            })
            .collect();
        self.open_picker(Picker::new("Headings in all documents", items, Source::Fuzzy));
    }

    pub(super) fn pick_definitions(
        &mut self,
        name: &str,
        candidates: &[u32],
    ) {
        let Some(index) = self.index.clone() else {
            return;
        };
        let items = candidates
            .iter()
            .map(|&c| {
                let s = index.symbol(c);
                Item {
                    label: format!("{} {}", s.kind.icon(), s.name),
                    detail: format!("{}  {}", self.node_summary(c), self.describe(s.loc))
                        .trim()
                        .to_owned(),
                    haystack: format!("{} {}", s.name, self.describe(s.loc)),
                    action: PickAction::Loc(s.loc, None),
                }
            })
            .collect();
        self.open_picker(Picker::new(format!("Definitions of “{name}”"), items, Source::Fuzzy));
    }

    pub(super) fn pick_references(
        &mut self,
        key: RefKey,
        occurrences: &[Occurrence],
    ) {
        let name = match key {
            (spec, WHOLE_SPEC) => self.lib.ids[spec as usize].clone(),
            (spec, section) => self.describe(Loc::section(spec, section)),
        };
        if occurrences.is_empty() {
            return self.info(format!("no references to {name}"));
        }
        let items = occurrences
            .iter()
            .take(MAX_REFERENCES)
            .filter_map(|o| {
                let entry = self.lib.entry(o.loc.spec)?.clone();
                let label = self.describe(o.loc);
                let detail = ref_snippet(&entry, o);
                Some(Item {
                    haystack: format!("{label} {detail}"),
                    label,
                    detail,
                    action: PickAction::Loc(o.loc, Some(o.nth)),
                })
            })
            .collect();
        self.open_picker(Picker::new(format!("References to {name} ({})", occurrences.len()), items, Source::Fuzzy));
    }

    pub(super) fn pick_marks(&mut self) {
        let spec = self.spec_id(self.view.key);
        let local = spec
            .as_ref()
            .and_then(|s| self.state.marks.get(s))
            .cloned()
            .unwrap_or_default();
        let marks: Vec<(char, Place)> = local
            .into_iter()
            .chain(self.state.global_marks.clone())
            .collect();
        let items = marks
            .into_iter()
            .map(|(c, place)| {
                let label = format!("{c}  {}", self.describe_place(&place));
                Item {
                    haystack: label.clone(),
                    label,
                    detail: String::new(),
                    action: PickAction::Place(place),
                }
            })
            .collect();
        self.open_picker(Picker::new("Marks", items, Source::Fuzzy));
    }

    pub(super) fn pick_jumps(&mut self) {
        let entries = self.jumps.entries.clone();
        let items = entries
            .into_iter()
            .rev()
            .map(|place| {
                let label = self.describe_place(&place);
                Item {
                    haystack: label.clone(),
                    label,
                    detail: String::new(),
                    action: PickAction::Place(place),
                }
            })
            .collect();
        self.open_picker(Picker::new("Jump list", items, Source::Fuzzy));
    }

    pub(super) fn pick_keymaps(&mut self) {
        let items = KEYMAP
            .iter()
            .map(|b| Item {
                label: b.keys.to_owned(),
                detail: b.desc.to_owned(),
                haystack: format!("{} {}", b.keys, b.desc),
                action: PickAction::Run(b.action),
            })
            .collect();
        self.open_picker(Picker::new(
            "Key bindings — <CR> runs one · :help lists commands with Tab",
            items,
            Source::Fuzzy,
        ));
    }

    pub(super) fn live_grep(
        &mut self,
        query: String,
    ) {
        if self.grep.is_none() {
            return self.info("still indexing the documents…");
        }
        let mut picker = Picker::new("Search all documents", Vec::new(), Source::Grep);
        picker.query = query;
        self.open_picker(picker);
        self.picker_query_changed();
    }

    pub(super) fn picker_query_changed(&mut self) {
        let Some(picker) = &mut self.picker else {
            return;
        };
        match picker.source {
            Source::Fuzzy => picker.refilter(),
            Source::Grep if picker.query.trim().len() < 2 => {
                picker.generation = u64::MAX;
                picker.set_items(Vec::new());
            }
            Source::Grep => {
                if let Some(grep) = &mut self.grep {
                    picker.generation = grep.submit(&picker.query);
                }
            }
        }
    }

    pub(super) fn poll_grep(&mut self) -> bool {
        let (Some(grep), Some(picker)) = (&self.grep, &self.picker) else {
            return false;
        };
        let Some((generation, hits)) = grep.poll() else {
            return false;
        };
        if picker.source != Source::Grep || generation != picker.generation {
            return false;
        }
        let items = hits
            .iter()
            .filter_map(|hit| {
                let entry = self.lib.entry(hit.loc.spec)?.clone();
                let label = self.describe(hit.loc);
                let detail = snippet(&slot_text(&entry, hit.loc), hit.offset, 12, 160);
                Some(Item {
                    haystack: String::new(),
                    label,
                    detail,
                    action: PickAction::Grep(hit.loc),
                })
            })
            .collect();
        if let Some(picker) = &mut self.picker {
            picker.set_items(items);
            picker.title = format!("Search all documents ({} hits)", hits.len());
        }
        true
    }

    pub(super) fn picker_key(
        &mut self,
        token: &str,
    ) {
        let half = (self.height / 2).max(1) as isize;
        let Some(picker) = &mut self.picker else {
            return;
        };
        let mut query_changed = false;
        match token {
            "<Esc>" | "<C-c>" => self.picker = None,
            "<CR>" => self.accept_pick(),
            "<Down>" | "<C-n>" | "<C-j>" | "<Tab>" => picker.move_by(1),
            "<Up>" | "<C-p>" | "<C-k>" | "<S-Tab>" => picker.move_by(-1),
            "<PageDown>" => picker.move_by(10),
            "<PageUp>" => picker.move_by(-10),
            "<C-d>" => picker.preview_scroll += half,
            "<C-u>" => picker.preview_scroll -= half,
            "<BS>" => query_changed = picker.query.pop().is_some(),
            "<C-w>" => {
                let trimmed = picker.query.trim_end();
                let keep = trimmed.rfind(' ').map_or(0, |i| i + 1);
                picker.query.truncate(keep);
                query_changed = true;
            }
            "<Space>" => {
                picker.query.push(' ');
                query_changed = true;
            }
            t if t.chars().count() == 1 => {
                picker.query.push_str(t);
                query_changed = true;
            }
            _ => {}
        }
        if query_changed {
            self.picker_query_changed();
        }
    }

    fn accept_pick(&mut self) {
        let Some(picker) = self.picker.take() else {
            return;
        };
        let Some(item) = picker.current().cloned() else {
            return;
        };
        match item.action {
            PickAction::Loc(loc, nth) => self.jump(|app| app.open_loc(loc, nth)),
            PickAction::Place(place) => self.jump(|app| {
                app.goto_place(&place);
            }),
            PickAction::Spec(idx) => self.jump(|app| app.open_remembered(idx)),
            PickAction::Run(action) => self.run(action, None),
            PickAction::Grep(loc) => {
                self.jump(|app| app.open_loc(loc, None));
                self.highlight_query(&picker.query);
            }
        }
    }

    fn open_remembered(
        &mut self,
        idx: SpecIdx,
    ) {
        let remembered = self
            .state
            .positions
            .contains_key(&self.lib.ids[idx as usize]);
        if remembered {
            self.switch(ViewKey::Spec(idx));
        } else {
            self.open_spec(idx, None);
        }
    }

    /// After a full-text hit, make `n`/`N` continue with the query and put the cursor on the first match.
    fn highlight_query(
        &mut self,
        query: &str,
    ) {
        let query = query.trim();
        let start = (self.view.cursor.line.saturating_sub(1), usize::MAX);
        let pattern = [query, query.split_whitespace().next().unwrap_or(query)]
            .into_iter()
            .find(|p| {
                self.find_match(p, start, true)
                    .is_some_and(|((l, _), _)| l <= self.view.cursor.line + 40)
            });
        if let Some(pattern) = pattern {
            if let Some(((line, col), _)) = self.find_match(pattern, start, true) {
                self.set_cursor(line, col);
            }
            self.search.pattern = Some(pattern.to_owned());
            self.search.forward = true;
            self.search.highlight = true;
        }
    }

    pub fn describe(
        &self,
        loc: Loc,
    ) -> String {
        let id = self
            .lib
            .ids
            .get(loc.spec as usize)
            .map_or("?", String::as_str);
        match self.lib.entry(loc.spec) {
            Some(entry) if (loc.section as usize) < entry.spec.sections.len() => {
                format!("{id} {}", entry.heading(loc.section))
            }
            _ => id.to_owned(),
        }
    }

    pub fn describe_place(
        &mut self,
        place: &Place,
    ) -> String {
        let Some(spec) = &place.spec else {
            return "home".to_owned();
        };
        let Some(idx) = self.lib.idx(spec) else {
            return spec.clone();
        };
        let _ = self.lib.load(idx);
        self.describe(Loc::section(idx, place.origin.section))
    }

    /// The section a picker item points at, rendered for the preview pane.
    pub fn preview(
        &mut self,
        width: usize,
    ) -> Option<(Arc<Doc>, usize, Option<String>)> {
        let picker = self.picker.as_ref()?;
        let item = picker.current()?.clone();
        let highlight = (picker.source == Source::Grep).then(|| picker.query.clone());
        let (loc, nth) = match &item.action {
            PickAction::Loc(loc, nth) => (*loc, *nth),
            PickAction::Grep(loc) => (*loc, None),
            PickAction::Spec(idx) => {
                let _ = self.lib.load(*idx);
                let section = self.lib.entry(*idx)?.labels.get("1").copied().unwrap_or(0);
                (Loc::section(*idx, section), None)
            }
            PickAction::Place(place) => {
                let idx = self.lib.idx(place.spec.as_deref()?)?;
                (
                    Loc {
                        spec: idx,
                        section: place.origin.section,
                        slot: place.origin.slot,
                    },
                    None,
                )
            }
            PickAction::Run(_) => return None,
        };
        let key = (loc.spec, loc.section, width);
        let doc = match &self.preview_cache {
            Some((cached, doc)) if *cached == key => doc.clone(),
            _ => {
                let entry = self.lib.load(loc.spec).ok()?;
                let opts = Opts {
                    width,
                    textwidth: width,
                };
                let doc = Arc::new(layout_section(&entry, loc.spec, self.index.as_deref(), opts, loc.section));
                self.preview_cache = Some((key, doc.clone()));
                doc
            }
        };
        let line = match nth.and_then(|n| doc.find_ref(loc.section, loc.slot, n)) {
            Some((line, _)) => line,
            None if loc.slot > 0 => doc.line_of(crate::doc::Origin {
                section: loc.section,
                slot: loc.slot,
                row: 0,
            }),
            None => 0,
        };
        Some((doc, line, highlight))
    }
}

/// Text around the `nth` reference of a slot, for the references list.
fn ref_snippet(
    entry: &SpecEntry,
    occurrence: &Occurrence,
) -> String {
    let loc = occurrence.loc;
    let text = slot_text(entry, loc);
    let block = entry.section(loc.section).blocks.get(loc.slot as usize - 1);
    let mut needle = None;
    let mut n = 0;
    if let Some(block) = block {
        for_each_ref(block, &mut |inline| {
            if n == occurrence.nth {
                needle = Some(match inline {
                    uareference_model::Inline::Italic(c) | uareference_model::Inline::Link { content: c, .. } => {
                        plain(c)
                    }
                    _ => String::new(),
                });
            }
            n += 1;
        });
    }
    let offset = needle.and_then(|needle| text.find(&needle)).unwrap_or(0);
    snippet(&text, offset, 12, 160)
}
