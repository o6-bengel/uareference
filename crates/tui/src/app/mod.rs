mod actions;
mod cmdline;
mod command;
pub mod keys;
pub mod motion;
pub mod picker;
mod sources;
pub mod state;
mod tabs;

use std::collections::HashMap;
use std::sync::{
    Arc,
    mpsc,
};
use std::thread;

use crossterm::event::{
    Event,
    KeyEvent,
    KeyEventKind,
    MouseButton,
    MouseEvent,
    MouseEventKind,
};
use uareference_model::Store;
use uareference_scrape::{
    SyncEvent,
    SyncReport,
};

use self::cmdline::CmdLine;
use self::keys::{
    Outcome,
    Pending,
};
use self::motion::{
    Cursor,
    Find,
    clamp_col,
};
use self::picker::Picker;
use self::state::{
    JumpList,
    Place,
    State,
};
use self::tabs::Tab;
use crate::corpus::{
    Library,
    Loc,
    SpecIdx,
    Target,
    load_all,
};
use crate::doc::layout::{
    Opts,
    layout_home,
    layout_spec,
};
use crate::doc::{
    Doc,
    Origin,
};
use crate::index::Index;
use crate::search::{
    Grep,
    SearchText,
};

const DOC_CACHE: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ViewKey {
    Home,
    Spec(SpecIdx),
}

pub struct View {
    pub key: ViewKey,
    pub doc: Arc<Doc>,
    pub cursor: Cursor,
    pub top: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Visual { line: bool, anchor: (usize, usize) },
}

#[derive(Debug, Default)]
pub struct Search {
    pub pattern: Option<String>,
    pub forward: bool,
    pub highlight: bool,
}

pub struct Message {
    pub text: String,
    pub error: bool,
}

pub struct Hover {
    pub title: String,
    pub doc: Doc,
    pub scroll: usize,
    pub target: Target,
}

pub struct Loaded {
    lib: Library,
    index: Arc<Index>,
    text: Arc<SearchText>,
}

enum SyncMsg {
    Event(SyncEvent),
    Done(Result<SyncReport, String>),
}

pub struct SyncJob {
    rx: mpsc::Receiver<SyncMsg>,
    pub done: usize,
    pub total: usize,
    pub current: String,
}

pub struct App {
    pub lib: Library,
    pub index: Option<Arc<Index>>,
    grep: Option<Grep>,
    loader: Option<mpsc::Receiver<Loaded>>,
    pub state: State,
    pub view: View,
    tabs: Vec<Tab>,
    pub tab: usize,
    alternate: Option<ViewKey>,
    docs: HashMap<ViewKey, Arc<Doc>>,
    jumps: JumpList,
    last_jump: Option<Place>,
    pub pending: Pending,
    pub mode: Mode,
    pub cmdline: Option<CmdLine>,
    pub picker: Option<Picker>,
    pub hover: Option<Hover>,
    pub search: Search,
    last_find: Option<(Find, char)>,
    pub message: Option<Message>,
    pub width: usize,
    pub height: usize,
    pub quit: bool,
    pub sync: Option<SyncJob>,
    pub redraw: bool,
    pub doc_rect: ratatui::layout::Rect,
    preview_cache: Option<((SpecIdx, u32, usize), Arc<Doc>)>,
}

impl App {
    pub fn new(store: Store) -> anyhow::Result<Self> {
        let lib = Library::open(store.clone())?;
        let state = State::load(&store);
        let loader = Some(spawn_loader(store));
        let view = View {
            key: ViewKey::Home,
            doc: Arc::new(Doc::default()),
            cursor: Cursor::default(),
            top: 0,
        };
        Ok(Self {
            lib,
            index: None,
            grep: None,
            loader,
            state,
            view,
            tabs: Vec::new(),
            tab: 0,
            alternate: None,
            docs: HashMap::new(),
            jumps: JumpList::default(),
            last_jump: None,
            pending: Pending::default(),
            mode: Mode::Normal,
            cmdline: None,
            picker: None,
            hover: None,
            search: Search {
                forward: true,
                ..Search::default()
            },
            last_find: None,
            message: None,
            width: 80,
            height: 24,
            quit: false,
            sync: None,
            redraw: false,
            doc_rect: ratatui::layout::Rect::default(),
            preview_cache: None,
        })
    }

    pub fn start(
        &mut self,
        spec: Option<&str>,
        label: Option<&str>,
    ) {
        self.view.doc = self.render(ViewKey::Home).unwrap_or_default();
        if let Some(spec) = spec {
            self.open_by_name(spec, label);
        } else if let Some(last) = self.state.last.clone() {
            self.goto_place(&last);
        }
    }

    pub fn is_indexing(&self) -> bool {
        self.loader.is_some()
    }

    pub fn resize(
        &mut self,
        width: usize,
        height: usize,
    ) {
        self.height = height.max(1);
        if width != self.width {
            self.width = width.max(20);
            self.rerender();
        }
        self.scroll_to_cursor();
    }

    fn opts(&self) -> Opts {
        Opts {
            width: self.width,
            textwidth: self.state.textwidth,
        }
    }

    pub fn rerender(&mut self) {
        let place = self.place();
        self.docs.clear();
        if let Ok(doc) = self.render(self.view.key) {
            self.view.doc = doc;
            let line = self.view.doc.line_of(place.origin);
            self.set_cursor(line, place.col);
        }
    }

    fn render(
        &mut self,
        key: ViewKey,
    ) -> anyhow::Result<Arc<Doc>> {
        if let Some(doc) = self.docs.get(&key) {
            return Ok(doc.clone());
        }
        let doc = match key {
            ViewKey::Home => layout_home(&self.lib, self.width),
            ViewKey::Spec(idx) => {
                let entry = self.lib.load(idx)?;
                layout_spec(&entry, idx, self.index.as_deref(), self.opts())
            }
        };
        if self.docs.len() >= DOC_CACHE {
            self.docs.clear();
        }
        let doc = Arc::new(doc);
        self.docs.insert(key, doc.clone());
        Ok(doc)
    }

    pub fn spec_id(
        &self,
        key: ViewKey,
    ) -> Option<String> {
        match key {
            ViewKey::Home => None,
            ViewKey::Spec(idx) => self.lib.ids.get(idx as usize).cloned(),
        }
    }

    pub fn current_spec(&self) -> Option<SpecIdx> {
        match self.view.key {
            ViewKey::Spec(idx) => Some(idx),
            ViewKey::Home => None,
        }
    }

    pub fn current_section(&self) -> Option<u32> {
        self.current_spec()
            .map(|_| self.view.doc.section_at(self.view.cursor.line))
    }

    pub fn place(&self) -> Place {
        let origin = self
            .view
            .doc
            .lines
            .get(self.view.cursor.line)
            .map(|l| l.origin)
            .unwrap_or_default();
        Place {
            spec: self.spec_id(self.view.key),
            origin,
            col: self.view.cursor.col,
        }
    }

    fn key_of(
        &self,
        spec: Option<&str>,
    ) -> Option<ViewKey> {
        match spec {
            None => Some(ViewKey::Home),
            Some(id) => self.lib.idx(id).map(ViewKey::Spec),
        }
    }

    pub fn goto_place(
        &mut self,
        place: &Place,
    ) -> bool {
        let Some(key) = self.key_of(place.spec.as_deref()) else {
            self.error(format!("{} is not synced", place.spec.as_deref().unwrap_or("?")));
            return false;
        };
        if !self.switch(key) {
            return false;
        }
        let line = self.view.doc.line_of(place.origin);
        self.set_cursor(line, place.col);
        true
    }

    fn switch(
        &mut self,
        key: ViewKey,
    ) -> bool {
        if self.view.key == key && !self.view.doc.lines.is_empty() {
            return true;
        }
        let doc = match self.render(key) {
            Ok(doc) => doc,
            Err(e) => {
                self.error(format!("{e:#}"));
                return false;
            }
        };
        self.remember_position();
        if self.view.key != key {
            self.alternate = Some(self.view.key);
        }
        let restore = self
            .spec_id(key)
            .and_then(|id| self.state.positions.get(&id).cloned());
        self.view = View {
            key,
            doc,
            cursor: Cursor::default(),
            top: 0,
        };
        self.mode = Mode::Normal;
        match restore {
            Some(place) => {
                let line = self.view.doc.line_of(place.origin);
                self.set_cursor(line, place.col);
            }
            None => self.set_cursor(0, 0),
        }
        true
    }

    fn remember_position(&mut self) {
        if let Some(id) = self.spec_id(self.view.key)
            && !self.view.doc.lines.is_empty()
        {
            let place = self.place();
            self.state.positions.insert(id, place);
        }
    }

    /// Runs a navigation and records where it started in the jump list when it moved anywhere.
    fn jump(
        &mut self,
        f: impl FnOnce(&mut Self),
    ) {
        let from = self.place();
        f(self);
        let to = self.place();
        if from.spec != to.spec || from.origin != to.origin {
            self.jumps.record(from.clone());
            self.last_jump = Some(from);
        }
    }

    pub fn open_loc(
        &mut self,
        loc: Loc,
        nth: Option<u32>,
    ) {
        if !self.switch(ViewKey::Spec(loc.spec)) {
            return;
        }
        let doc = &self.view.doc;
        let found = nth.and_then(|n| doc.find_ref(loc.section, loc.slot, n));
        let (line, col) = match found {
            Some(found) => found,
            None if loc.slot == 0 => (doc.heading_line(loc.section).unwrap_or(0), 0),
            None => (
                doc.line_of(Origin {
                    section: loc.section,
                    slot: loc.slot,
                    row: 0,
                }),
                0,
            ),
        };
        self.set_cursor(line, col);
        if nth.is_none() {
            self.view.cursor.col = motion::first_non_blank(&self.view.doc, line);
            self.view.cursor.want = self.view.cursor.col;
            self.scroll_cursor_top();
        }
    }

    pub fn open_spec(
        &mut self,
        idx: SpecIdx,
        label: Option<&str>,
    ) {
        let _ = self.lib.load(idx);
        let section = self.lib.entry(idx).and_then(|e| {
            let labels = &e.labels;
            label
                .and_then(|l| labels.get(l))
                .or_else(|| labels.get("1"))
                .copied()
        });
        match section {
            Some(section) => self.open_loc(Loc::section(idx, section), None),
            None => {
                self.switch(ViewKey::Spec(idx));
            }
        }
        let missing = label.filter(|l| {
            self.lib
                .entry(idx)
                .is_none_or(|e| !e.labels.contains_key(*l))
        });
        if let Some(label) = missing {
            let id = self.lib.ids[idx as usize].clone();
            self.error(format!("no clause {label} in {id}"));
        }
    }

    /// Opens a document named loosely (`10000-4`, `opc 10000-4`, or part of its title) at an optional clause.
    pub fn open_by_name(
        &mut self,
        name: &str,
        label: Option<&str>,
    ) {
        let idx = uareference_model::text::normalize_spec_id(name)
            .and_then(|id| self.lib.idx(&id))
            .or_else(|| {
                let needle = name.to_lowercase();
                (0..self.lib.ids.len()).map(|i| i as SpecIdx).find(|&i| {
                    self.lib.title(i).to_lowercase().contains(&needle)
                        || self.lib.ids[i as usize].to_lowercase().contains(&needle)
                })
            });
        match idx {
            Some(idx) => self.jump(|app| app.open_spec(idx, label)),
            None => self.error(format!("no document matches `{name}`")),
        }
    }

    pub fn go_target(
        &mut self,
        target: Target,
    ) {
        match target {
            Target::Loc(loc) => self.jump(|app| app.open_loc(loc, None)),
            Target::Spec(idx) => self.jump(|app| app.open_spec(idx, None)),
            Target::Url(url) => self.open_url(&url),
            Target::Missing(why) => self.error(why),
        }
    }

    pub fn set_cursor(
        &mut self,
        line: usize,
        col: usize,
    ) {
        let doc = &self.view.doc;
        let line = line.min(doc.len().saturating_sub(1));
        let col = clamp_col(doc, line, col);
        self.view.cursor = Cursor { line, col, want: col };
        self.scroll_to_cursor();
    }

    fn move_line(
        &mut self,
        line: usize,
    ) {
        let doc = &self.view.doc;
        let line = line.min(doc.len().saturating_sub(1));
        let want = self.view.cursor.want;
        self.view.cursor.line = line;
        self.view.cursor.col = clamp_col(doc, line, want);
        self.scroll_to_cursor();
    }

    /// Lines kept visible around the cursor; at half the window or more the cursor line stays centered.
    fn scrolloff(&self) -> usize {
        self.state.scrolloff.min(self.height.saturating_sub(1) / 2)
    }

    fn centered(&self) -> bool {
        self.state.scrolloff >= self.height / 2
    }

    fn scroll_to_cursor(&mut self) {
        let line = self.view.cursor.line;
        if self.centered() {
            self.view.top = line.saturating_sub(self.height / 2).min(self.max_top());
            return;
        }
        let so = self.scrolloff();
        if line < self.view.top + so {
            self.view.top = line.saturating_sub(so);
        }
        if line + so >= self.view.top + self.height {
            self.view.top = (line + so + 1).saturating_sub(self.height);
        }
        self.view.top = self.view.top.min(self.max_top());
    }

    fn max_top(&self) -> usize {
        self.view.doc.len().saturating_sub(1)
    }

    fn scroll_cursor_top(&mut self) {
        if self.centered() {
            return self.scroll_to_cursor();
        }
        let so = self.scrolloff();
        self.view.top = self.view.cursor.line.saturating_sub(so).min(self.max_top());
    }

    pub fn info(
        &mut self,
        text: impl Into<String>,
    ) {
        self.message = Some(Message {
            text: text.into(),
            error: false,
        });
    }

    pub fn error(
        &mut self,
        text: impl Into<String>,
    ) {
        self.message = Some(Message {
            text: text.into(),
            error: true,
        });
    }

    pub fn save(&mut self) {
        self.remember_position();
        self.state.last = Some(self.place());
        if let Err(e) = self.state.save(&self.lib.store) {
            eprintln!("saving state: {e:#}");
        }
    }

    /// Drains background channels; returns whether anything changed on screen.
    pub fn tick(&mut self) -> bool {
        let mut changed = self.poll_loader();
        changed |= self.poll_grep();
        changed |= self.poll_sync();
        changed
    }

    fn poll_loader(&mut self) -> bool {
        let Some(rx) = &self.loader else {
            return false;
        };
        let loaded = match rx.try_recv() {
            Ok(loaded) => loaded,
            Err(mpsc::TryRecvError::Empty) => return false,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.loader = None;
                self.error("loading the documents failed");
                return true;
            }
        };
        self.loader = None;
        self.install(loaded);
        true
    }

    fn install(
        &mut self,
        loaded: Loaded,
    ) {
        let place = self.place();
        let old = std::mem::replace(&mut self.lib, loaded.lib);
        self.rekey_alternates(&old);
        self.index = Some(loaded.index);
        self.grep = Some(Grep::spawn(loaded.text));
        self.docs.clear();
        self.view.key = self.key_of(place.spec.as_deref()).unwrap_or(ViewKey::Home);
        if let Ok(doc) = self.render(self.view.key) {
            self.view.doc = doc;
            let line = self.view.doc.line_of(place.origin);
            self.set_cursor(line, place.col);
        }
    }

    pub fn reload(&mut self) {
        match Library::open(self.lib.store.clone()) {
            Ok(lib) => {
                self.lib = lib;
                self.index = None;
                self.grep = None;
                self.loader = Some(spawn_loader(self.lib.store.clone()));
                self.rerender();
            }
            Err(e) => self.error(format!("{e:#}")),
        }
    }

    fn poll_sync(&mut self) -> bool {
        let Some(job) = &mut self.sync else {
            return false;
        };
        let mut finished = None;
        let mut changed = false;
        while let Ok(msg) = job.rx.try_recv() {
            changed = true;
            match msg {
                SyncMsg::Event(SyncEvent::Catalog { total }) => job.total = total,
                SyncMsg::Event(
                    SyncEvent::Skipped { id } | SyncEvent::Parsed { id, .. } | SyncEvent::Failed { id, .. },
                ) => {
                    job.done += 1;
                    job.current = id;
                }
                SyncMsg::Event(SyncEvent::Downloaded { id, .. }) => job.current = id,
                SyncMsg::Done(result) => finished = Some(result),
            }
        }
        if let Some(result) = finished {
            self.sync = None;
            match result {
                Ok(report) => {
                    let failed = report
                        .failed
                        .iter()
                        .map(|(id, e)| format!("{id}: {e}"))
                        .collect::<Vec<_>>();
                    let summary = format!("sync: {} updated, {} unchanged", report.updated.len(), report.skipped.len());
                    if failed.is_empty() {
                        self.info(summary);
                    } else {
                        self.error(format!("{summary}, {} failed — {}", failed.len(), failed.join("; ")));
                    }
                    if !report.updated.is_empty() || self.lib.ids.is_empty() {
                        self.reload();
                    }
                }
                Err(e) => self.error(format!("sync failed: {e}")),
            }
        }
        changed
    }

    pub fn start_sync(
        &mut self,
        only: Vec<String>,
        force: bool,
    ) {
        if self.sync.is_some() {
            self.error("a sync is already running");
            return;
        }
        let (tx, rx) = mpsc::channel();
        let store = self.lib.store.clone();
        thread::spawn(move || {
            let options = uareference_scrape::SyncOptions {
                only,
                force,
                reparse: false,
                jobs: 4,
            };
            let on_event = |event: SyncEvent| {
                let _ = tx.send(SyncMsg::Event(event));
            };
            let result = uareference_scrape::sync(&store, &options, &on_event).map_err(|e| e.to_string());
            let _ = tx.send(SyncMsg::Done(result));
        });
        self.sync = Some(SyncJob {
            rx,
            done: 0,
            total: 0,
            current: String::new(),
        });
        self.info("sync started");
    }

    pub fn handle(
        &mut self,
        event: Event,
    ) {
        match event {
            Event::Key(key) if key.kind != KeyEventKind::Release => self.key(key),
            Event::Mouse(mouse) => self.mouse(mouse),
            Event::Paste(text) => {
                if let Some(cmdline) = &mut self.cmdline {
                    cmdline.insert_str(&text);
                } else if let Some(picker) = &mut self.picker {
                    picker.query.push_str(&text);
                    self.picker_query_changed();
                }
            }
            _ => {}
        }
    }

    fn key(
        &mut self,
        key: KeyEvent,
    ) {
        let Some(token) = keys::token(key) else {
            return;
        };
        if self.picker.is_some() {
            return self.picker_key(&token);
        }
        if self.cmdline.is_some() {
            return self.cmdline_key(&token);
        }
        if self.hover.is_some() && self.hover_key(&token) {
            return;
        }
        if self.pending.keys.is_empty() && self.pending.count.is_none() {
            self.message = None;
        }
        if let Mode::Visual { .. } = self.mode
            && self.pending.keys.is_empty()
            && self.visual_key(&token)
        {
            return;
        }
        match self.pending.feed(&token) {
            Outcome::Run(action, count) => self.run(action, count),
            Outcome::Wait => {}
            Outcome::Unmapped(keys) => {
                if keys != "<Esc>" {
                    self.error(format!("{keys} is not mapped — <Space>? lists the key bindings"));
                }
            }
        }
    }

    fn mouse(
        &mut self,
        mouse: MouseEvent,
    ) {
        if self.picker.is_some() || self.cmdline.is_some() {
            return;
        }
        match mouse.kind {
            MouseEventKind::ScrollDown => self.scroll_lines(3, true),
            MouseEventKind::ScrollUp => self.scroll_lines(3, false),
            MouseEventKind::Down(MouseButton::Left) => {
                self.hover = None;
                if let Some((line, col)) = crate::ui::doc_position(self, mouse.column, mouse.row) {
                    self.set_cursor(line, col);
                }
            }
            _ => {}
        }
    }

    fn scroll_lines(
        &mut self,
        n: usize,
        down: bool,
    ) {
        if let Some(hover) = &mut self.hover {
            hover.scroll = if down {
                hover.scroll + n
            } else {
                hover.scroll.saturating_sub(n)
            };
            hover.scroll = hover.scroll.min(hover.doc.len().saturating_sub(1));
            return;
        }
        let line = self.view.cursor.line;
        if self.centered() {
            return self.move_line(if down { line + n } else { line.saturating_sub(n) });
        }
        let max = self.max_top();
        self.view.top = if down {
            (self.view.top + n).min(max)
        } else {
            self.view.top.saturating_sub(n)
        };
        let so = self.scrolloff();
        if line < self.view.top + so {
            self.move_line((self.view.top + so).min(self.view.doc.len().saturating_sub(1)));
        } else if line + so >= self.view.top + self.height {
            self.move_line((self.view.top + self.height).saturating_sub(so + 1));
        }
    }
}

fn spawn_loader(store: Store) -> mpsc::Receiver<Loaded> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let Ok(mut lib) = Library::open(store) else {
            return;
        };
        lib.entries = load_all(&lib.store, &lib.ids);
        let index = Arc::new(Index::build(&lib));
        let text = Arc::new(SearchText::build(&lib));
        let _ = tx.send(Loaded { lib, index, text });
    });
    rx
}
