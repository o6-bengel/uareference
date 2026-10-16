use super::state::{
    JumpList,
    Place,
};
use super::{
    App,
    Mode,
    ViewKey,
};
use crate::corpus::Library;

/// A background tab; the active one lives in `App::view`, `alternate`, `jumps` and `last_jump`.
pub(super) struct Tab {
    place: Place,
    row: usize,
    alternate: Option<ViewKey>,
    jumps: JumpList,
    last_jump: Option<Place>,
}

impl App {
    pub fn tab_count(&self) -> usize {
        self.tabs.len() + 1
    }

    /// The document each tab shows, in order; `None` is the home page.
    pub fn tab_specs(&self) -> Vec<Option<String>> {
        let mut specs: Vec<_> = self.tabs.iter().map(|t| t.place.spec.clone()).collect();
        specs.insert(self.tab, self.spec_id(self.view.key));
        specs
    }

    pub(super) fn new_tab(&mut self) {
        let current = self.snapshot();
        self.tabs.insert(self.tab, current);
        self.tab += 1;
    }

    pub(super) fn close_tab(&mut self) {
        if self.tabs.is_empty() {
            return self.error("cannot close the last tab");
        }
        let i = self.tab.min(self.tabs.len() - 1);
        let next = self.tabs.remove(i);
        self.tab = i;
        self.restore(next);
    }

    pub(super) fn cycle_tab(
        &mut self,
        delta: isize,
    ) {
        let i = (self.tab as isize + delta).rem_euclid(self.tab_count() as isize);
        self.goto_tab(i as usize);
    }

    pub(super) fn goto_tab(
        &mut self,
        i: usize,
    ) {
        if i == self.tab {
            return;
        }
        let current = self.snapshot();
        self.tabs.insert(self.tab, current);
        let target = self.tabs.remove(i);
        self.tab = i;
        self.restore(target);
    }

    /// Maps every tab's alternate document from the indices of `old` to those of the current library.
    pub(super) fn rekey_alternates(
        &mut self,
        old: &Library,
    ) {
        let lib = &self.lib;
        let rekey = |key: Option<ViewKey>| match key? {
            ViewKey::Home => Some(ViewKey::Home),
            ViewKey::Spec(idx) => lib.idx(old.ids.get(idx as usize)?).map(ViewKey::Spec),
        };
        self.alternate = rekey(self.alternate);
        for tab in &mut self.tabs {
            tab.alternate = rekey(tab.alternate);
        }
    }

    fn snapshot(&self) -> Tab {
        Tab {
            place: self.place(),
            row: self.view.cursor.line.saturating_sub(self.view.top),
            alternate: self.alternate,
            jumps: self.jumps.clone(),
            last_jump: self.last_jump.clone(),
        }
    }

    fn restore(
        &mut self,
        tab: Tab,
    ) {
        self.mode = Mode::Normal;
        self.hover = None;
        if !self.goto_place(&tab.place) {
            self.switch(ViewKey::Home);
        }
        self.view.top = self.view.cursor.line.saturating_sub(tab.row);
        self.scroll_to_cursor();
        self.alternate = tab.alternate;
        self.jumps = tab.jumps;
        self.last_jump = tab.last_jump;
    }
}
