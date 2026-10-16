use uareference_model::text::normalize_spec_id;

use super::App;
use crate::corpus::{
    Loc,
    Target,
};

pub const COMMANDS: &[&str] = &[
    "def",
    "delmarks",
    "edit",
    "grep",
    "headings",
    "help",
    "home",
    "jumps",
    "marks",
    "node",
    "nohlsearch",
    "outline",
    "quit",
    "refs",
    "reload",
    "set",
    "specs",
    "symbols",
    "sync",
    "terms",
    "toc",
    "web",
];

impl App {
    pub(super) fn execute(
        &mut self,
        line: &str,
    ) {
        let line = line.trim().trim_start_matches(':');
        if line.is_empty() {
            return;
        }
        if is_clause_label(line) {
            return self.goto_clause(line);
        }
        let (cmd, args) = line
            .split_once(char::is_whitespace)
            .map_or((line, ""), |(c, a)| (c, a.trim()));
        let (cmd, bang) = cmd.strip_suffix('!').map_or((cmd, false), |c| (c, true));
        match cmd {
            "q" | "qa" | "qall" | "quit" | "wq" | "x" | "xa" => self.quit = true,
            "e" | "edit" | "o" | "open" | "b" | "buffer" => self.edit(args),
            "def" | "d" | "definition" => self.define(args),
            "node" | "nodeid" => self.node(args),
            "grep" | "g" | "rg" | "search" => self.live_grep(args.to_owned()),
            "refs" | "references" => self.run(super::keys::Action::References, None),
            "marks" => self.pick_marks(),
            "delm" | "delmarks" => self.delete_marks(args, bang),
            "jumps" | "ju" => self.pick_jumps(),
            "toc" => self.state.outline = !self.state.outline,
            "outline" => self.pick_outline(),
            "noh" | "nohlsearch" => self.search.highlight = false,
            "set" | "se" => self.set_option(args),
            "sync" => self.sync_command(args, bang),
            "web" | "browse" | "browser" => self.run(super::keys::Action::OpenBrowser, None),
            "help" | "h" | "keys" | "map" => self.pick_keymaps(),
            "home" => self.run(super::keys::Action::Home, None),
            "specs" | "ls" | "buffers" => self.pick_specs(),
            "symbols" | "sym" => self.pick_symbols(false),
            "terms" => self.pick_symbols(true),
            "headings" => self.pick_headings(),
            "reload" => self.reload(),
            _ => self.error(format!("not a command: {cmd} — try :help")),
        }
    }

    fn edit(
        &mut self,
        args: &str,
    ) {
        let mut parts = args.split_whitespace();
        match (parts.next(), parts.next()) {
            (None, _) => self.pick_specs(),
            (Some(spec), label) => self.open_by_name(spec, label),
        }
    }

    fn goto_clause(
        &mut self,
        label: &str,
    ) {
        let Some(spec) = self.current_spec() else {
            return self.error("open a document first");
        };
        let section = self
            .lib
            .entry(spec)
            .and_then(|e| e.labels.get(label).copied());
        match section {
            Some(section) => self.jump(|app| app.open_loc(Loc::section(spec, section), None)),
            None => self.error(format!("no clause {label} here")),
        }
    }

    fn define(
        &mut self,
        name: &str,
    ) {
        let Some(index) = self.index.clone() else {
            return self.info("still indexing the documents…");
        };
        let candidates = index.candidates(name, self.current_spec().unwrap_or(0));
        match candidates.as_slice() {
            [] => self.error(format!("no definition of “{name}”")),
            [only] => self.go_target(Target::Loc(index.symbol(*only).loc)),
            _ => self.pick_definitions(name, &candidates),
        }
    }

    fn node(
        &mut self,
        id: &str,
    ) {
        let Some(index) = self.index.clone() else {
            return self.info("still indexing the documents…");
        };
        let found = index.by_node_id(id).to_vec();
        match found.as_slice() {
            [] => self.error(format!("no node {id}")),
            [only] => self.go_target(Target::Loc(index.symbol(*only).loc)),
            _ => self.pick_definitions(id, &found),
        }
    }

    fn delete_marks(
        &mut self,
        args: &str,
        all: bool,
    ) {
        let spec = self.spec_id(self.view.key);
        if all {
            if let Some(spec) = spec {
                self.state.marks.remove(&spec);
            }
            return;
        }
        for c in args.chars().filter(char::is_ascii_alphabetic) {
            self.state.delete_mark(c, spec.as_deref());
        }
    }

    fn set_option(
        &mut self,
        args: &str,
    ) {
        for arg in args.split_whitespace() {
            match arg.split_once('=') {
                Some(("tw" | "textwidth", value)) => match value.parse::<usize>() {
                    Ok(tw) if tw >= 20 => {
                        self.state.textwidth = tw;
                        self.rerender();
                    }
                    _ => self.error(format!("invalid textwidth: {value}")),
                },
                Some(("so" | "scrolloff", value)) => match value.parse::<usize>() {
                    Ok(so) => {
                        self.state.scrolloff = so;
                        self.scroll_to_cursor();
                    }
                    _ => self.error(format!("invalid scrolloff: {value}")),
                },
                None if arg == "outline" => self.state.outline = true,
                None if arg == "nooutline" => self.state.outline = false,
                None if arg == "tw?" || arg == "textwidth?" => self.info(format!("textwidth={}", self.state.textwidth)),
                _ => self.error(format!("unknown option: {arg}")),
            }
        }
    }

    fn sync_command(
        &mut self,
        args: &str,
        force: bool,
    ) {
        let only: Vec<String> = args
            .split_whitespace()
            .filter_map(normalize_spec_id)
            .collect();
        self.start_sync(only, force);
    }
}

/// `5.7.2`, `12`, `A`, `B.3.9`: a clause of the current document rather than a command.
fn is_clause_label(text: &str) -> bool {
    let mut parts = text.split('.');
    let first = parts.next().unwrap_or_default();
    let head_ok = first.chars().all(|c| c.is_ascii_digit()) && !first.is_empty()
        || first.len() == 1 && first.chars().all(|c| c.is_ascii_uppercase());
    head_ok && parts.all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}
