use super::App;
use super::command::COMMANDS;
use super::motion::Cursor;
use super::state::remember;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmdKind {
    Command,
    Search { forward: bool },
}

#[derive(Debug, Clone)]
pub struct CmdLine {
    pub kind: CmdKind,
    pub text: String,
    /// Byte offset of the editing cursor in `text`.
    pub cursor: usize,
    origin: Cursor,
    origin_top: Option<usize>,
    history: Option<usize>,
    completion: Option<(Vec<String>, usize, String)>,
}

impl CmdLine {
    pub fn new(
        kind: CmdKind,
        origin: Cursor,
    ) -> Self {
        Self {
            kind,
            text: String::new(),
            cursor: 0,
            origin,
            origin_top: None,
            history: None,
            completion: None,
        }
    }

    pub fn prompt(&self) -> char {
        match self.kind {
            CmdKind::Command => ':',
            CmdKind::Search { forward: true } => '/',
            CmdKind::Search { forward: false } => '?',
        }
    }

    pub fn insert_str(
        &mut self,
        s: &str,
    ) {
        let s = s.replace(['\n', '\r'], " ");
        self.text.insert_str(self.cursor, &s);
        self.cursor += s.len();
    }

    fn set(
        &mut self,
        text: String,
    ) {
        self.cursor = text.len();
        self.text = text;
    }

    fn backspace(&mut self) {
        if let Some((i, _)) = self.text[..self.cursor].char_indices().last() {
            self.text.remove(i);
            self.cursor = i;
        }
    }

    fn delete_word(&mut self) {
        let before = self.text[..self.cursor].trim_end();
        let start = before.rfind(' ').map_or(0, |i| i + 1);
        self.text.replace_range(start..self.cursor, "");
        self.cursor = start;
    }

    fn left(&mut self) {
        if let Some((i, _)) = self.text[..self.cursor].char_indices().last() {
            self.cursor = i;
        }
    }

    fn right(&mut self) {
        if let Some(c) = self.text[self.cursor..].chars().next() {
            self.cursor += c.len_utf8();
        }
    }
}

impl App {
    pub(super) fn cmdline_key(
        &mut self,
        token: &str,
    ) {
        let Some(mut cl) = self.cmdline.take() else {
            return;
        };
        if cl.origin_top.is_none() {
            cl.origin_top = Some(self.view.top);
        }
        if token != "<Tab>" {
            cl.completion = None;
        }
        match token {
            "<Esc>" | "<C-c>" => return self.cancel_cmdline(&cl),
            "<CR>" => return self.submit_cmdline(cl),
            "<BS>" if cl.text.is_empty() => return self.cancel_cmdline(&cl),
            "<BS>" => cl.backspace(),
            "<C-u>" => cl.set(String::new()),
            "<C-w>" => cl.delete_word(),
            "<Left>" => cl.left(),
            "<Right>" => cl.right(),
            "<Home>" | "<C-a>" => cl.cursor = 0,
            "<End>" | "<C-e>" => cl.cursor = cl.text.len(),
            "<Up>" | "<C-p>" => self.history(&mut cl, true),
            "<Down>" | "<C-n>" => self.history(&mut cl, false),
            "<Tab>" => self.complete(&mut cl),
            "<Space>" => cl.insert_str(" "),
            t if t.chars().count() == 1 => cl.insert_str(t),
            _ => {}
        }
        if let CmdKind::Search { forward } = cl.kind {
            self.incsearch(&cl, forward);
        }
        self.cmdline = Some(cl);
    }

    fn cancel_cmdline(
        &mut self,
        cl: &CmdLine,
    ) {
        if matches!(cl.kind, CmdKind::Search { .. }) {
            self.view.cursor = cl.origin;
            self.view.top = cl.origin_top.unwrap_or(self.view.top);
        }
    }

    fn submit_cmdline(
        &mut self,
        cl: CmdLine,
    ) {
        match cl.kind {
            CmdKind::Command => {
                remember(&mut self.state.command_history, cl.text.trim());
                self.execute(&cl.text);
            }
            CmdKind::Search { forward } => {
                self.view.cursor = cl.origin;
                if !cl.text.is_empty() {
                    remember(&mut self.state.search_history, &cl.text);
                    self.search.pattern = Some(cl.text.clone());
                }
                self.search.forward = forward;
                self.search_next(1, forward);
            }
        }
    }

    fn incsearch(
        &mut self,
        cl: &CmdLine,
        forward: bool,
    ) {
        if cl.text.is_empty() {
            self.view.cursor = cl.origin;
            return;
        }
        let from = (cl.origin.line, cl.origin.col);
        if let Some(((line, col), _)) = self.find_match(&cl.text, from, forward) {
            self.set_cursor(line, col);
        }
    }

    fn history(
        &self,
        cl: &mut CmdLine,
        older: bool,
    ) {
        let history = match cl.kind {
            CmdKind::Command => &self.state.command_history,
            CmdKind::Search { .. } => &self.state.search_history,
        };
        if history.is_empty() {
            return;
        }
        let next = match (cl.history, older) {
            (None, true) => Some(history.len() - 1),
            (None, false) => None,
            (Some(0), true) => Some(0),
            (Some(i), true) => Some(i - 1),
            (Some(i), false) if i + 1 < history.len() => Some(i + 1),
            (Some(_), false) => None,
        };
        cl.history = next;
        cl.set(next.map(|i| history[i].clone()).unwrap_or_default());
    }

    fn complete(
        &self,
        cl: &mut CmdLine,
    ) {
        if cl.kind != CmdKind::Command {
            return;
        }
        if let Some((options, i, base)) = &mut cl.completion {
            *i = (*i + 1) % options.len();
            let text = format!("{base}{}", options[*i]);
            cl.set(text);
            return;
        }
        let (base, word, options) = match cl.text.split_once(' ') {
            None => (String::new(), cl.text.clone(), COMMANDS.iter().map(|c| c.to_string()).collect::<Vec<_>>()),
            Some((cmd, arg)) if matches!(cmd, "e" | "edit" | "o" | "open" | "b" | "sync") => (
                format!("{cmd} "),
                arg.to_owned(),
                self.lib
                    .catalog
                    .entries
                    .iter()
                    .map(|e| e.id.clone())
                    .collect(),
            ),
            Some((cmd, arg)) if cmd == "set" => (
                format!("{cmd} "),
                arg.to_owned(),
                ["textwidth=", "scrolloff=", "outline", "nooutline"]
                    .map(str::to_owned)
                    .to_vec(),
            ),
            _ => return,
        };
        let lower = word.to_lowercase();
        let matching: Vec<String> = options
            .into_iter()
            .filter(|o| o.to_lowercase().contains(&lower))
            .collect();
        match matching.len() {
            0 => {}
            1 => cl.set(format!("{base}{}", matching[0])),
            _ => {
                cl.set(format!("{base}{}", matching[0]));
                cl.completion = Some((matching, 0, base));
            }
        }
    }
}
