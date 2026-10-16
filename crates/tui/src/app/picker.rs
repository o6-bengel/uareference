use nucleo_matcher::pattern::{
    CaseMatching,
    Normalization,
    Pattern,
};
use nucleo_matcher::{
    Config,
    Matcher,
    Utf32Str,
};

use super::keys::Action;
use super::state::Place;
use crate::corpus::{
    Loc,
    SpecIdx,
};

#[derive(Debug, Clone)]
pub enum PickAction {
    Loc(Loc, Option<u32>),
    Place(Place),
    Spec(SpecIdx),
    Run(Action),
    Grep(Loc),
}

#[derive(Debug, Clone)]
pub struct Item {
    pub label: String,
    pub detail: String,
    pub haystack: String,
    pub action: PickAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Fuzzy,
    Grep,
}

pub struct Picker {
    pub title: String,
    pub source: Source,
    pub items: Vec<Item>,
    pub filtered: Vec<u32>,
    pub query: String,
    pub selected: usize,
    pub preview_scroll: isize,
    pub generation: u64,
    matcher: Matcher,
}

impl Picker {
    pub fn new(
        title: impl Into<String>,
        items: Vec<Item>,
        source: Source,
    ) -> Self {
        let mut picker = Self {
            title: title.into(),
            source,
            items,
            filtered: Vec::new(),
            query: String::new(),
            selected: 0,
            preview_scroll: 0,
            generation: 0,
            matcher: Matcher::new(Config::DEFAULT),
        };
        picker.refilter();
        picker
    }

    pub fn select_item(
        mut self,
        item: usize,
    ) -> Self {
        if let Some(pos) = self.filtered.iter().position(|&i| i as usize == item) {
            self.selected = pos;
        }
        self
    }

    pub fn set_items(
        &mut self,
        items: Vec<Item>,
    ) {
        self.items = items;
        self.refilter();
    }

    pub fn refilter(&mut self) {
        self.selected = 0;
        self.preview_scroll = 0;
        if self.source == Source::Grep || self.query.trim().is_empty() {
            self.filtered = (0..self.items.len() as u32).collect();
            return;
        }
        let query = self.query.trim();
        if looks_like_node_id(query) {
            let exact = |item: &Item| item.haystack.split_whitespace().any(|token| token == query);
            self.filtered = (0..self.items.len() as u32)
                .filter(|&i| exact(&self.items[i as usize]))
                .collect();
            return;
        }
        let pattern = Pattern::parse(&self.query, CaseMatching::Smart, Normalization::Smart);
        let mut buf = Vec::new();
        let mut scored: Vec<(u32, u32)> = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(i, item)| {
                let score = pattern.score(Utf32Str::new(&item.haystack, &mut buf), &mut self.matcher)?;
                Some((score, i as u32))
            })
            .collect();
        scored.sort_by_key(|&(score, i)| (std::cmp::Reverse(score), i));
        self.filtered = scored.into_iter().map(|(_, i)| i).collect();
    }

    pub fn current(&self) -> Option<&Item> {
        self.items.get(*self.filtered.get(self.selected)? as usize)
    }

    pub fn move_by(
        &mut self,
        delta: isize,
    ) {
        let len = self.filtered.len();
        if len == 0 {
            return;
        }
        self.selected = (self.selected as isize + delta).rem_euclid(len as isize) as usize;
        self.preview_scroll = 0;
    }
}

fn looks_like_node_id(query: &str) -> bool {
    ["i=", "s=", "g=", "b=", "ns=", "nsu="]
        .iter()
        .any(|p| query.starts_with(p))
}
