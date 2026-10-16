use uareference_model::{
    Block,
    NodeRole,
};

use super::cmdline::{
    CmdKind,
    CmdLine,
};
use super::keys::Action;
use super::motion::{
    Cursor,
    Find,
    matches_in,
};
use super::{
    App,
    Hover,
    Mode,
    ViewKey,
    motion,
};
use crate::corpus::{
    Loc,
    Target,
};
use crate::doc::layout::{
    Opts,
    layout_section,
};
use crate::doc::{
    Doc,
    Line,
    LinkKind,
    LinkRef,
    Origin,
    Role,
    Span,
    Sty,
};
use crate::index::{
    WHOLE_SPEC,
    short_node_id,
};
use crate::platform;

/// Start and end (line, column), inclusive, and whether the selection is linewise.
pub type Selection = ((usize, usize), (usize, usize), bool);

pub enum Subject {
    Link(LinkRef),
    Word(String),
}

impl Subject {
    /// The name to look up in the symbol index, for terms and plain words.
    pub fn name(&self) -> Option<&str> {
        match self {
            Subject::Link(LinkRef {
                kind: LinkKind::Term { text, .. },
                ..
            }) => Some(text),
            Subject::Word(word) => Some(word),
            Subject::Link(_) => None,
        }
    }
}

impl App {
    pub(super) fn run(
        &mut self,
        action: Action,
        count: Option<usize>,
    ) {
        if action.is_motion() {
            return self.motion(action, count);
        }
        if matches!(self.mode, Mode::Visual { .. }) && !matches!(action, Action::VisualChar | Action::VisualLine) {
            self.mode = Mode::Normal;
        }
        let n = count.unwrap_or(1);
        match action {
            Action::Follow | Action::Definition => self.definition(),
            Action::Definitions => self.definitions(),
            Action::References => self.references(),
            Action::Hover => self.show_hover(),
            Action::OpenBrowser => self.open_in_browser(),
            Action::YankLine => self.yank_lines(n),
            Action::YankUrl => self.yank_url(),
            Action::YankCitation => self.yank_citation(),
            Action::YankNodeId => self.yank_node_id(),
            Action::VisualChar => self.toggle_visual(false),
            Action::VisualLine => self.toggle_visual(true),
            Action::JumpBack => self.jump_back(n),
            Action::JumpForward => self.jump_forward(n),
            Action::Alternate => self.alternate(),
            Action::Home => self.jump(|app| {
                app.switch(ViewKey::Home);
            }),
            Action::TabNew => self.new_tab(),
            Action::TabClose => self.close_tab(),
            Action::TabNext => self.cycle_tab(n as isize),
            Action::TabPrev => self.cycle_tab(-(n as isize)),
            Action::TabFirst => self.goto_tab(0),
            Action::TabLast => self.goto_tab(self.tab_count() - 1),
            Action::SetMark(c) => self.set_mark(c),
            Action::GotoMark(c, exact) => self.goto_mark(c, exact),
            Action::SearchForward => {
                self.cmdline = Some(CmdLine::new(CmdKind::Search { forward: true }, self.view.cursor))
            }
            Action::SearchBackward => {
                self.cmdline = Some(CmdLine::new(CmdKind::Search { forward: false }, self.view.cursor))
            }
            Action::StarForward => self.star(true),
            Action::StarBackward => self.star(false),
            Action::ClearHighlight => {
                self.search.highlight = false;
                self.hover = None;
            }
            Action::PickSpecs => self.pick_specs(),
            Action::PickOutline => self.pick_outline(),
            Action::PickSymbols => self.pick_symbols(false),
            Action::PickTerms => self.pick_symbols(true),
            Action::PickHeadings => self.pick_headings(),
            Action::LiveGrep => self.live_grep(String::new()),
            Action::GrepWord => {
                let word = motion::word_at(&self.view.doc, self.view.cursor.line, self.view.cursor.col);
                self.live_grep(word.unwrap_or_default());
            }
            Action::PickMarks => self.pick_marks(),
            Action::PickJumps => self.pick_jumps(),
            Action::PickKeymaps => self.pick_keymaps(),
            Action::ToggleOutline => self.state.outline = !self.state.outline,
            Action::Command => self.cmdline = Some(CmdLine::new(CmdKind::Command, self.view.cursor)),
            Action::Info => self.show_info(),
            Action::Redraw => self.redraw = true,
            Action::Quit => self.quit = true,
            _ => {}
        }
    }

    fn motion(
        &mut self,
        action: Action,
        count: Option<usize>,
    ) {
        let n = count.unwrap_or(1);
        let doc = self.view.doc.clone();
        let Cursor { line, col, .. } = self.view.cursor;
        let last = doc.len().saturating_sub(1);
        let half = (self.height / 2).max(1);
        match action {
            Action::Left => self.set_cursor(line, step(&doc, line, col, -(n as isize))),
            Action::Right => self.set_cursor(line, step(&doc, line, col, n as isize)),
            Action::Down => self.move_line(line + n),
            Action::Up => self.move_line(line.saturating_sub(n)),
            Action::WordForward(big) => self.repeat(n, |d, l, c| motion::word_forward(d, l, c, big)),
            Action::WordBack(big) => self.repeat(n, |d, l, c| motion::word_back(d, l, c, big)),
            Action::WordEnd(big) => self.repeat(n, |d, l, c| motion::word_end(d, l, c, big)),
            Action::LineStart => self.set_cursor(line, 0),
            Action::FirstNonBlank => self.set_cursor(line, motion::first_non_blank(&doc, line)),
            Action::LineEnd => {
                self.set_cursor(line, motion::last_col(&doc, line));
                self.view.cursor.want = usize::MAX;
            }
            Action::Top => self.jump_line(count.map_or(0, |c| c - 1)),
            Action::Bottom => self.jump_line(count.map_or(last, |c| c - 1)),
            Action::Percent => self.jump_line((n.min(100) * doc.len()).div_ceil(100).saturating_sub(1)),
            Action::ScreenTop => self.jump_line(self.view.top + if self.view.top == 0 { 0 } else { self.scrolloff() }),
            Action::ScreenMiddle => self.jump_line(self.view.top + (self.height.min(doc.len() - self.view.top)) / 2),
            Action::ScreenBottom => self.jump_line(
                (self.view.top + self.height)
                    .saturating_sub(self.scrolloff() + 1)
                    .min(last),
            ),
            Action::ParagraphForward => self.jump_to_line((0..n).fold(line, |l, _| motion::paragraph_forward(&doc, l))),
            Action::ParagraphBack => self.jump_to_line((0..n).fold(line, |l, _| motion::paragraph_back(&doc, l))),
            Action::NextSection => self.section_motion(n, true),
            Action::PrevSection => self.section_motion(n, false),
            Action::ParentSection => self.parent_section(),
            Action::NextTable => self.block_motion(n, true, |b| matches!(b, Block::Table(_))),
            Action::PrevTable => self.block_motion(n, false, |b| matches!(b, Block::Table(_))),
            Action::NextFigure => self.block_motion(n, true, |b| matches!(b, Block::Figure { .. })),
            Action::PrevFigure => self.block_motion(n, false, |b| matches!(b, Block::Figure { .. })),
            Action::NextLink => self.repeat(n, |d, l, c| motion::next_link(d, l, c, true).unwrap_or((l, c))),
            Action::PrevLink => self.repeat(n, |d, l, c| motion::next_link(d, l, c, false).unwrap_or((l, c))),
            Action::FindChar(find, c) => {
                self.last_find = Some((find, c));
                self.find_char(find, c, n);
            }
            Action::RepeatFind => {
                if let Some((find, c)) = self.last_find {
                    self.find_char(find, c, n);
                }
            }
            Action::RepeatFindReverse => {
                if let Some((find, c)) = self.last_find {
                    self.find_char(find.reversed(), c, n);
                }
            }
            Action::HalfPageDown => self.scroll_with_cursor(half * n, true),
            Action::HalfPageUp => self.scroll_with_cursor(half * n, false),
            Action::PageDown => self.scroll_with_cursor(self.height.saturating_sub(2).max(1) * n, true),
            Action::PageUp => self.scroll_with_cursor(self.height.saturating_sub(2).max(1) * n, false),
            Action::ScrollDown => self.scroll_lines(n, true),
            Action::ScrollUp => self.scroll_lines(n, false),
            Action::ScrollTop => self.scroll_view(line),
            Action::ScrollCenter => self.scroll_view(line.saturating_sub(self.height / 2)),
            Action::ScrollBottom => self.scroll_view((line + 1).saturating_sub(self.height)),
            Action::SearchNext => self.search_next(n, self.search.forward),
            Action::SearchPrev => self.search_next(n, !self.search.forward),
            _ => {}
        }
    }

    /// `zt`, `zz`, `zb`: the scroll offset still applies, as in Vim.
    fn scroll_view(
        &mut self,
        top: usize,
    ) {
        self.view.top = top.min(self.max_top());
        self.scroll_to_cursor();
    }

    fn repeat(
        &mut self,
        n: usize,
        f: impl Fn(&Doc, usize, usize) -> (usize, usize),
    ) {
        let doc = self.view.doc.clone();
        let (mut line, mut col) = (self.view.cursor.line, self.view.cursor.col);
        for _ in 0..n {
            (line, col) = f(&doc, line, col);
        }
        self.set_cursor(line, col);
    }

    fn jump_line(
        &mut self,
        line: usize,
    ) {
        self.jump(|app| {
            let line = line.min(app.view.doc.len().saturating_sub(1));
            let col = motion::first_non_blank(&app.view.doc, line);
            app.set_cursor(line, col);
        });
    }

    fn jump_to_line(
        &mut self,
        line: usize,
    ) {
        self.jump(|app| app.set_cursor(line, 0));
    }

    fn scroll_with_cursor(
        &mut self,
        lines: usize,
        down: bool,
    ) {
        let max = self.max_top();
        let line = self.view.cursor.line;
        if down {
            self.view.top = (self.view.top + lines).min(max);
            self.move_line(line + lines);
        } else {
            self.view.top = self.view.top.saturating_sub(lines);
            self.move_line(line.saturating_sub(lines));
        }
    }

    fn section_motion(
        &mut self,
        n: usize,
        forward: bool,
    ) {
        let doc = self.view.doc.clone();
        let mut lines: Vec<usize> = doc.headings.iter().flatten().map(|&l| l as usize).collect();
        lines.sort_unstable();
        let line = self.view.cursor.line;
        let target = if forward {
            lines.iter().filter(|&&l| l > line).nth(n - 1).copied()
        } else {
            lines
                .iter()
                .rev()
                .filter(|&&l| l < line)
                .nth(n - 1)
                .copied()
        };
        let target = target.unwrap_or(if forward { doc.len().saturating_sub(1) } else { 0 });
        self.jump(|app| {
            app.set_cursor(target, motion::first_non_blank(&app.view.doc, target));
            app.scroll_cursor_top();
        });
    }

    fn parent_section(&mut self) {
        let (Some(spec), Some(section)) = (self.current_spec(), self.current_section()) else {
            return;
        };
        let Some(entry) = self.lib.entry(spec) else {
            return;
        };
        let heading = self.view.doc.heading_line(section);
        let target = match (heading, entry.section(section).parent) {
            (Some(h), _) if h < self.view.cursor.line => section,
            (_, Some(parent)) => parent,
            _ => return,
        };
        self.jump(|app| app.open_loc(Loc::section(spec, target), None));
    }

    fn block_motion(
        &mut self,
        n: usize,
        forward: bool,
        kind: impl Fn(&Block) -> bool,
    ) {
        let Some(entry) = self.current_spec().and_then(|s| self.lib.entry(s)).cloned() else {
            return;
        };
        let doc = self.view.doc.clone();
        let is_start = |l: usize| {
            let o = doc.lines[l].origin;
            o.slot > 0
                && o.row == 0
                && entry
                    .section(o.section)
                    .blocks
                    .get(o.slot as usize - 1)
                    .is_some_and(&kind)
        };
        let line = self.view.cursor.line;
        let found = if forward {
            (line + 1..doc.len()).filter(|&l| is_start(l)).nth(n - 1)
        } else {
            (0..line).rev().filter(|&l| is_start(l)).nth(n - 1)
        };
        match found {
            Some(target) => self.jump(|app| app.set_cursor(target, motion::first_non_blank(&app.view.doc, target))),
            None => self.error("no more"),
        }
    }

    fn find_char(
        &mut self,
        find: Find,
        c: char,
        n: usize,
    ) {
        let Cursor { line, col, .. } = self.view.cursor;
        if let Some(col) = motion::find_char(&self.view.doc, line, col, find, c, n) {
            self.set_cursor(line, col);
        }
    }

    pub(super) fn search_next(
        &mut self,
        n: usize,
        forward: bool,
    ) {
        let Some(pattern) = self.search.pattern.clone() else {
            return self.error("no previous search pattern");
        };
        self.search.highlight = true;
        let mut at = (self.view.cursor.line, self.view.cursor.col);
        let mut wrapped = false;
        for _ in 0..n {
            match self.find_match(&pattern, at, forward) {
                Some((pos, w)) => {
                    at = pos;
                    wrapped |= w;
                }
                None => return self.error(format!("pattern not found: {pattern}")),
            }
        }
        self.jump(|app| app.set_cursor(at.0, at.1));
        let prefix = if forward { '/' } else { '?' };
        match wrapped {
            true => self.error(format!(
                "search hit {}, continuing at {}",
                if forward { "BOTTOM" } else { "TOP" },
                if forward { "TOP" } else { "BOTTOM" }
            )),
            false => self.info(format!("{prefix}{pattern}")),
        }
    }

    /// Next match strictly after (or before) a position, wrapping around; also says whether it wrapped.
    pub(super) fn find_match(
        &self,
        pattern: &str,
        (line, col): (usize, usize),
        forward: bool,
    ) -> Option<((usize, usize), bool)> {
        let doc = &self.view.doc;
        let len = doc.len();
        for i in 0..=len {
            let l = if forward {
                (line + i) % len.max(1)
            } else {
                (line + len * 2 - i) % len.max(1)
            };
            let wrapped = if forward { line + i >= len } else { i > line };
            let found = matches_in(&doc.lines.get(l)?.text(), pattern);
            let hit = if forward {
                found
                    .into_iter()
                    .map(|(s, _)| s)
                    .find(|&s| i > 0 || s > col)
            } else {
                found
                    .into_iter()
                    .map(|(s, _)| s)
                    .rev()
                    .find(|&s| i > 0 || s < col)
            };
            if let Some(c) = hit {
                return Some(((l, c), wrapped));
            }
        }
        None
    }

    fn star(
        &mut self,
        forward: bool,
    ) {
        let Some(word) = motion::word_at(&self.view.doc, self.view.cursor.line, self.view.cursor.col) else {
            return self.error("no word under the cursor");
        };
        let pattern = format!("\\<{word}\\>");
        super::state::remember(&mut self.state.search_history, &pattern);
        self.search.pattern = Some(pattern);
        self.search.forward = forward;
        self.search_next(1, forward);
    }

    fn toggle_visual(
        &mut self,
        line: bool,
    ) {
        self.mode = match self.mode {
            Mode::Visual { line: current, .. } if current == line => Mode::Normal,
            Mode::Visual { anchor, .. } => Mode::Visual { line, anchor },
            Mode::Normal => Mode::Visual {
                line,
                anchor: (self.view.cursor.line, self.view.cursor.col),
            },
        };
    }

    /// Keys with a meaning of their own in visual mode; returns whether the key was consumed.
    pub(super) fn visual_key(
        &mut self,
        token: &str,
    ) -> bool {
        let Mode::Visual { line, anchor } = self.mode else {
            return false;
        };
        match token {
            "y" => {
                let text = self.selection_text();
                self.mode = Mode::Normal;
                let lines = text.lines().count();
                self.yank(&text, &format!("{lines} line(s)"));
            }
            "o" => {
                let cursor = (self.view.cursor.line, self.view.cursor.col);
                self.mode = Mode::Visual { line, anchor: cursor };
                self.set_cursor(anchor.0, anchor.1);
            }
            "<Esc>" => self.mode = Mode::Normal,
            _ => return false,
        }
        true
    }

    pub fn selection(&self) -> Option<Selection> {
        let Mode::Visual { line, anchor } = self.mode else {
            return None;
        };
        let cursor = (self.view.cursor.line, self.view.cursor.col);
        let (start, end) = if anchor <= cursor {
            (anchor, cursor)
        } else {
            (cursor, anchor)
        };
        Some((start, end, line))
    }

    fn selection_text(&self) -> String {
        let Some((start, end, linewise)) = self.selection() else {
            return String::new();
        };
        let doc = &self.view.doc;
        let mut out = Vec::new();
        for l in start.0..=end.0 {
            let text = doc.lines[l].text();
            let piece = if linewise {
                text
            } else {
                let from = if l == start.0 { start.1 } else { 0 };
                let to = if l == end.0 { end.1 + 1 } else { usize::MAX };
                slice_cols(&text, from, to)
            };
            out.push(
                piece
                    .strip_prefix(' ')
                    .unwrap_or(&piece)
                    .trim_end()
                    .to_owned(),
            );
        }
        out.join("\n")
    }

    fn yank_lines(
        &mut self,
        n: usize,
    ) {
        let doc = &self.view.doc;
        let start = self.view.cursor.line;
        let end = (start + n).min(doc.len());
        let text: Vec<String> = (start..end)
            .map(|l| doc.lines[l].text().trim().to_owned())
            .collect();
        self.yank(&text.join("\n"), &format!("{} line(s)", end - start));
    }

    fn yank_url(&mut self) {
        match self.section_url() {
            Some(url) => self.yank(&url.clone(), &url),
            None => self.error("no section here"),
        }
    }

    fn yank_citation(&mut self) {
        let (Some(spec), Some(section)) = (self.current_spec(), self.current_section()) else {
            return self.error("no section here");
        };
        let Some(entry) = self.lib.entry(spec).cloned() else {
            return;
        };
        let s = entry.section(section);
        let citation = match &s.label {
            Some(label) => format!("{}, {label} {}", entry.spec.document, s.title),
            None => format!("{}, {}", entry.spec.document, s.title),
        };
        self.yank(&citation, &citation);
    }

    fn yank_node_id(&mut self) {
        let from_link = self
            .subject()
            .as_ref()
            .and_then(Subject::name)
            .and_then(|name| {
                let index = self.index.as_ref()?;
                let symbol = index.symbol(index.resolve(name, self.current_spec()?)?);
                symbol.node_id.clone()
            });
        let from_section = || {
            let entry = self.lib.entry(self.current_spec()?)?;
            let section = entry.section(self.current_section()?);
            section
                .nodes
                .iter()
                .find(|n| matches!(n.role, NodeRole::Type | NodeRole::Object))
                .and_then(|n| n.node_id.clone())
        };
        match from_link.or_else(from_section) {
            Some(id) => self.yank(&id, &id),
            None => self.error("no NodeId here"),
        }
    }

    pub(super) fn yank(
        &mut self,
        text: &str,
        what: &str,
    ) {
        match platform::copy(text) {
            Ok(()) => self.info(format!("yanked {what}")),
            Err(e) => self.error(format!("yank failed: {e}")),
        }
    }

    fn section_url(&self) -> Option<String> {
        let entry = self.lib.entry(self.current_spec()?)?;
        Some(entry.url(self.current_section()))
    }

    fn jump_back(
        &mut self,
        n: usize,
    ) {
        let mut target = None;
        let mut current = self.place();
        for _ in 0..n {
            match self.jumps.back(current.clone()) {
                Some(place) => {
                    current = place.clone();
                    target = Some(place);
                }
                None => break,
            }
        }
        match target {
            Some(place) => {
                self.goto_place(&place);
            }
            None => self.error("at the start of the jump list"),
        }
    }

    fn jump_forward(
        &mut self,
        n: usize,
    ) {
        let target = (0..n).filter_map(|_| self.jumps.forward()).last();
        match target {
            Some(place) => {
                self.goto_place(&place);
            }
            None => self.error("at the end of the jump list"),
        }
    }

    fn alternate(&mut self) {
        match self.alternate {
            Some(key) => self.jump(|app| {
                app.switch(key);
            }),
            None => self.error("no alternate document"),
        }
    }

    fn set_mark(
        &mut self,
        c: char,
    ) {
        if !c.is_ascii_alphabetic() {
            return self.error(format!("invalid mark `{c}`: use a-z or A-Z"));
        }
        let place = self.place();
        self.state.set_mark(c, place);
        self.info(format!("mark {c} set"));
    }

    fn goto_mark(
        &mut self,
        c: char,
        exact: bool,
    ) {
        let place = if matches!(c, '\'' | '`') {
            self.last_jump.clone()
        } else {
            self.state
                .mark(c, self.spec_id(self.view.key).as_deref())
                .cloned()
        };
        let Some(place) = place else {
            return self.error(format!("mark {c} is not set"));
        };
        self.jump(|app| {
            if app.goto_place(&place) && !exact {
                let line = app.view.cursor.line;
                app.set_cursor(line, motion::first_non_blank(&app.view.doc, line));
            }
        });
    }

    /// What `gd`, `K` and `gr` act on: the link or term under the cursor, else the word under it.
    pub fn subject(&self) -> Option<Subject> {
        if let Some(link) = self.link_under_cursor() {
            return Some(Subject::Link(link));
        }
        motion::word_under(&self.view.doc, self.view.cursor.line, self.view.cursor.col).map(Subject::Word)
    }

    pub(super) fn subject_target(
        &mut self,
        subject: &Subject,
    ) -> Target {
        match subject {
            Subject::Link(link) => self.link_target(link),
            Subject::Word(word) => self.resolve_name(word),
        }
    }

    fn resolve_name(
        &self,
        name: &str,
    ) -> Target {
        let Some(index) = &self.index else {
            return Target::Missing("still indexing the documents…".to_owned());
        };
        match index.resolve(name, self.current_spec().unwrap_or(0)) {
            Some(symbol) => Target::Loc(index.symbol(symbol).loc),
            None => Target::Missing(format!("no definition of “{name}”")),
        }
    }

    pub fn link_under_cursor(&self) -> Option<LinkRef> {
        let id = self
            .view
            .doc
            .link_at(self.view.cursor.line, self.view.cursor.col)?;
        self.view.doc.links.get(id as usize).cloned()
    }

    /// Where a rendered link points; terms resolve against the index from the current document.
    pub(super) fn link_target(
        &mut self,
        link: &LinkRef,
    ) -> Target {
        let ctx = self.current_spec().unwrap_or(0);
        match &link.kind {
            LinkKind::Term { text, .. } => self.resolve_name(text),
            LinkKind::Link(target) => {
                if let uareference_model::Link::Spec { spec, .. } = target
                    && let Some(idx) = self.lib.idx(spec)
                {
                    let _ = self.lib.load(idx);
                }
                self.lib.resolve_link(ctx, target)
            }
            LinkKind::Spec(idx) => Target::Spec(*idx),
            LinkKind::Figure { url } => Target::Url(url.clone()),
        }
    }

    fn definition(&mut self) {
        let Some(subject) = self.subject() else {
            return self.error("no link, term or word under the cursor");
        };
        let ctx = self.current_spec();
        let target = self.subject_target(&subject);
        self.go_target(target);
        if let (Some(name), Some(index), Some(ctx)) = (subject.name(), &self.index, ctx) {
            let n = index.candidates(name, ctx).len();
            if n > 1 && self.message.is_none() {
                self.info(format!("1 of {n} definitions — gD lists them all"));
            }
        }
    }

    fn definitions(&mut self) {
        let Some(subject) = self.subject() else {
            return self.error("no term or word under the cursor");
        };
        let Some(name) = subject.name() else {
            return self.definition();
        };
        let Some(index) = self.index.clone() else {
            return self.info("still indexing the documents…");
        };
        let candidates = index.candidates(name, self.current_spec().unwrap_or(0));
        if candidates.is_empty() {
            return self.error(format!("no definition of “{name}”"));
        }
        self.pick_definitions(name, &candidates);
    }

    fn references(&mut self) {
        let Some(index) = self.index.clone() else {
            return self.info("still indexing the documents…");
        };
        let on_heading = self
            .view
            .doc
            .lines
            .get(self.view.cursor.line)
            .is_some_and(|l| l.origin.slot == 0);
        let subject = if on_heading { None } else { self.subject() };
        let key = match subject {
            Some(subject) => match self.subject_target(&subject) {
                Target::Loc(loc) => (loc.spec, loc.section),
                Target::Spec(idx) => (idx, WHOLE_SPEC),
                Target::Url(_) => return self.error("external links have no references"),
                Target::Missing(why) => return self.error(why),
            },
            None => match (self.current_spec(), self.current_section()) {
                (Some(spec), Some(section)) => (spec, section),
                _ => return self.error("nothing to find references to here"),
            },
        };
        let occurrences = index.references(key).to_vec();
        self.pick_references(key, &occurrences);
    }

    fn show_hover(&mut self) {
        let Some(subject) = self.subject() else {
            return self.error("no link, term or word under the cursor");
        };
        let target = self.subject_target(&subject);
        let width = self.width.saturating_sub(6).clamp(30, 96);
        let opts = Opts {
            width,
            textwidth: width,
        };
        let (loc, title) = match &target {
            Target::Loc(loc) => (*loc, None),
            Target::Spec(idx) => {
                let _ = self.lib.load(*idx);
                let Some(section) = self
                    .lib
                    .entry(*idx)
                    .and_then(|e| e.labels.get("1").copied())
                else {
                    return self.info(self.lib.title(*idx).to_owned());
                };
                (
                    Loc::section(*idx, section),
                    Some(format!("{} — {}", self.lib.ids[*idx as usize], self.lib.title(*idx))),
                )
            }
            Target::Url(url) => return self.info(url.clone()),
            Target::Missing(why) => return self.error(why.clone()),
        };
        let Ok(entry) = self.lib.load(loc.spec) else {
            return self.error("document not available");
        };
        let doc = layout_section(&entry, loc.spec, self.index.as_deref(), opts, loc.section);
        let scroll = match loc.slot {
            0 => 0,
            slot => doc.line_of(Origin {
                section: loc.section,
                slot,
                row: 0,
            }),
        };
        let title = title.unwrap_or_else(|| format!("{} {}", entry.spec.id, entry.heading(loc.section)));
        self.hover = Some(Hover {
            title,
            doc,
            scroll,
            target,
        });
    }

    /// Keys while the hover popup is open; returns whether the key was consumed.
    pub(super) fn hover_key(
        &mut self,
        token: &str,
    ) -> bool {
        let Some(hover) = &mut self.hover else {
            return false;
        };
        let page = (self.height / 2).max(1);
        let max = hover.doc.len().saturating_sub(1);
        match token {
            "j" | "<Down>" | "<C-e>" => hover.scroll = (hover.scroll + 1).min(max),
            "k" | "<Up>" | "<C-y>" => hover.scroll = hover.scroll.saturating_sub(1),
            "<C-d>" | "<C-f>" | "<PageDown>" => hover.scroll = (hover.scroll + page).min(max),
            "<C-u>" | "<C-b>" | "<PageUp>" => hover.scroll = hover.scroll.saturating_sub(page),
            "<CR>" | "K" => {
                let target = hover.target.clone();
                self.hover = None;
                self.go_target(target);
            }
            "<Esc>" | "q" => self.hover = None,
            _ => {
                self.hover = None;
                return false;
            }
        }
        true
    }

    fn open_in_browser(&mut self) {
        let url = match self.subject().map(|subject| self.subject_target(&subject)) {
            Some(Target::Url(url)) => Some(url),
            Some(Target::Loc(loc)) => self.lib.entry(loc.spec).map(|e| e.url(Some(loc.section))),
            Some(Target::Spec(idx)) => self.lib.entry(idx).map(|e| e.url(None)),
            Some(Target::Missing(_)) | None => self.section_url(),
        };
        match url {
            Some(url) => self.open_url(&url),
            None => self.error("nothing to open"),
        }
    }

    pub fn open_url(
        &mut self,
        url: &str,
    ) {
        match platform::open_url(url) {
            Ok(()) => self.info(format!("opened {url}")),
            Err(e) => self.error(format!("could not open {url}: {e}")),
        }
    }

    fn show_info(&mut self) {
        let line = self.view.cursor.line + 1;
        let total = self.view.doc.len().max(1);
        let Some(spec) = self.current_spec() else {
            return self.info(format!("home — {} documents", self.lib.ids.len()));
        };
        let id = self.lib.ids[spec as usize].clone();
        let entry = self.lib.catalog.get(&id);
        let version = entry.map_or("?", |e| e.version.as_str()).to_owned();
        let published = entry
            .and_then(|e| e.published.as_deref())
            .unwrap_or("?")
            .to_owned();
        let section = self
            .current_section()
            .zip(self.lib.entry(spec))
            .map(|(s, e)| e.heading(s))
            .unwrap_or_default();
        let url = self.section_url().unwrap_or_default();
        let rows = [
            ("Title", self.lib.title(spec).to_owned()),
            ("Version", version),
            ("Published", published),
            ("Section", section),
            ("Line", format!("{line} of {total} ({}%)", line * 100 / total)),
            ("URL", url.clone()),
        ];
        self.hover = Some(Hover {
            title: id,
            doc: info_doc(&rows),
            scroll: 0,
            target: Target::Url(url),
        });
    }

    pub fn node_summary(
        &self,
        symbol: u32,
    ) -> String {
        let Some(index) = &self.index else {
            return String::new();
        };
        let symbol = index.symbol(symbol);
        let class = symbol.node_class.as_deref().unwrap_or_default();
        let id = symbol
            .node_id
            .as_deref()
            .map(short_node_id)
            .unwrap_or_default();
        format!("{class} {id}").trim().to_owned()
    }
}

fn info_doc(rows: &[(&str, String)]) -> Doc {
    let key_width = rows.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
    let lines = rows
        .iter()
        .enumerate()
        .map(|(row, (key, value))| Line {
            spans: vec![
                Span::new(format!(" {key:<key_width$}  "), Sty::role(Role::Muted), None),
                Span::new(format!("{value} "), Sty::default(), None),
            ],
            origin: Origin {
                section: 0,
                slot: 0,
                row: row as u32,
            },
        })
        .collect();
    Doc {
        lines,
        ..Doc::default()
    }
}

fn step(
    doc: &Doc,
    line: usize,
    col: usize,
    delta: isize,
) -> usize {
    let cs = motion::chars(doc, line);
    let i = cs.iter().rposition(|&(_, c)| c <= col).unwrap_or(0) as isize;
    let j = (i + delta).clamp(0, cs.len().saturating_sub(1) as isize) as usize;
    cs.get(j).map_or(0, |&(_, c)| c)
}

fn slice_cols(
    text: &str,
    from: usize,
    to: usize,
) -> String {
    let mut col = 0;
    let mut out = String::new();
    for ch in text.chars() {
        if col >= from && col < to {
            out.push(ch);
        }
        col += unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
    }
    out
}
