use std::collections::BTreeMap;

use serde::{
    Deserialize,
    Serialize,
};
use uareference_model::Store;
use uareference_model::store::{
    read_json,
    write_json,
};

use crate::doc::Origin;

/// A stable position: survives re-wrapping because it names the source slot and row rather than a screen line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Place {
    /// `None` is the home page.
    pub spec: Option<String>,
    pub origin: Origin,
    pub col: usize,
}

impl Place {
    pub fn same_spot(
        &self,
        other: &Place,
    ) -> bool {
        self.spec == other.spec && self.origin.section == other.origin.section && self.origin.slot == other.origin.slot
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    pub last: Option<Place>,
    pub positions: BTreeMap<String, Place>,
    pub marks: BTreeMap<String, BTreeMap<char, Place>>,
    pub global_marks: BTreeMap<char, Place>,
    pub search_history: Vec<String>,
    pub command_history: Vec<String>,
    pub outline: bool,
    pub textwidth: usize,
    pub scrolloff: usize,
}

impl Default for State {
    fn default() -> Self {
        Self {
            last: None,
            positions: BTreeMap::new(),
            marks: BTreeMap::new(),
            global_marks: BTreeMap::new(),
            search_history: Vec::new(),
            command_history: Vec::new(),
            outline: true,
            textwidth: 100,
            scrolloff: 999,
        }
    }
}

const HISTORY: usize = 200;

impl State {
    pub fn load(store: &Store) -> Self {
        read_json(&store.state_path()).unwrap_or_default()
    }

    pub fn save(
        &self,
        store: &Store,
    ) -> anyhow::Result<()> {
        Ok(write_json(&store.state_path(), self)?)
    }

    pub fn set_mark(
        &mut self,
        mark: char,
        place: Place,
    ) {
        match &place.spec {
            Some(spec) if mark.is_ascii_lowercase() => {
                self.marks
                    .entry(spec.clone())
                    .or_default()
                    .insert(mark, place);
            }
            _ => {
                self.global_marks.insert(mark, place);
            }
        }
    }

    pub fn mark(
        &self,
        mark: char,
        spec: Option<&str>,
    ) -> Option<&Place> {
        if mark.is_ascii_lowercase() {
            self.marks.get(spec?)?.get(&mark)
        } else {
            self.global_marks.get(&mark)
        }
    }

    pub fn delete_mark(
        &mut self,
        mark: char,
        spec: Option<&str>,
    ) {
        if mark.is_ascii_lowercase() {
            if let Some(marks) = spec.and_then(|s| self.marks.get_mut(s)) {
                marks.remove(&mark);
            }
        } else {
            self.global_marks.remove(&mark);
        }
    }
}

pub fn remember(
    history: &mut Vec<String>,
    entry: &str,
) {
    if entry.is_empty() {
        return;
    }
    history.retain(|h| h != entry);
    history.push(entry.to_owned());
    if history.len() > HISTORY {
        history.remove(0);
    }
}

/// Browser-style history: a new jump drops everything after the current entry.
#[derive(Debug, Default, Clone)]
pub struct JumpList {
    pub entries: Vec<Place>,
    pub index: usize,
}

impl JumpList {
    pub fn record(
        &mut self,
        from: Place,
    ) {
        self.entries.truncate(self.index.min(self.entries.len()));
        if self
            .entries
            .last()
            .is_some_and(|last| last.same_spot(&from))
        {
            self.entries.pop();
        }
        self.entries.push(from);
        if self.entries.len() > 200 {
            self.entries.remove(0);
        }
        self.index = self.entries.len();
    }

    pub fn back(
        &mut self,
        current: Place,
    ) -> Option<Place> {
        if self.index >= self.entries.len() {
            if self
                .entries
                .last()
                .is_some_and(|last| last.same_spot(&current))
            {
                self.entries.pop();
            }
            self.entries.push(current);
            self.index = self.entries.len() - 1;
        }
        if self.index == 0 {
            return None;
        }
        self.index -= 1;
        Some(self.entries[self.index].clone())
    }

    pub fn forward(&mut self) -> Option<Place> {
        if self.index + 1 >= self.entries.len() {
            return None;
        }
        self.index += 1;
        Some(self.entries[self.index].clone())
    }
}
