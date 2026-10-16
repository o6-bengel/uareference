use crossterm::event::{
    KeyCode,
    KeyEvent,
    KeyModifiers,
};

use super::motion::Find;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Left,
    Right,
    Up,
    Down,
    WordForward(bool),
    WordBack(bool),
    WordEnd(bool),
    LineStart,
    FirstNonBlank,
    LineEnd,
    Top,
    Bottom,
    Percent,
    ScreenTop,
    ScreenMiddle,
    ScreenBottom,
    ParagraphForward,
    ParagraphBack,
    NextSection,
    PrevSection,
    ParentSection,
    NextTable,
    PrevTable,
    NextFigure,
    PrevFigure,
    NextLink,
    PrevLink,
    FindChar(Find, char),
    RepeatFind,
    RepeatFindReverse,
    HalfPageDown,
    HalfPageUp,
    PageDown,
    PageUp,
    ScrollDown,
    ScrollUp,
    ScrollTop,
    ScrollCenter,
    ScrollBottom,
    Follow,
    Definition,
    Definitions,
    References,
    Hover,
    OpenBrowser,
    YankLine,
    YankUrl,
    YankCitation,
    YankNodeId,
    VisualChar,
    VisualLine,
    JumpBack,
    JumpForward,
    Alternate,
    Home,
    TabNew,
    TabClose,
    TabNext,
    TabPrev,
    TabFirst,
    TabLast,
    SetMark(char),
    GotoMark(char, bool),
    SearchForward,
    SearchBackward,
    SearchNext,
    SearchPrev,
    StarForward,
    StarBackward,
    ClearHighlight,
    PickSpecs,
    PickOutline,
    PickSymbols,
    PickTerms,
    PickHeadings,
    LiveGrep,
    GrepWord,
    PickMarks,
    PickJumps,
    PickKeymaps,
    ToggleOutline,
    Command,
    Info,
    Redraw,
    Quit,
}

impl Action {
    pub fn is_motion(self) -> bool {
        use Action::*;
        matches!(
            self,
            Left | Right
                | Up
                | Down
                | WordForward(_)
                | WordBack(_)
                | WordEnd(_)
                | LineStart
                | FirstNonBlank
                | LineEnd
                | Top
                | Bottom
                | Percent
                | ScreenTop
                | ScreenMiddle
                | ScreenBottom
                | ParagraphForward
                | ParagraphBack
                | NextSection
                | PrevSection
                | ParentSection
                | NextTable
                | PrevTable
                | NextFigure
                | PrevFigure
                | NextLink
                | PrevLink
                | FindChar(..)
                | RepeatFind
                | RepeatFindReverse
                | HalfPageDown
                | HalfPageUp
                | PageDown
                | PageUp
                | ScrollDown
                | ScrollUp
                | ScrollTop
                | ScrollCenter
                | ScrollBottom
                | SearchNext
                | SearchPrev
        )
    }
}

pub struct Binding {
    pub keys: &'static str,
    pub action: Action,
    pub desc: &'static str,
}

const fn bind(
    keys: &'static str,
    action: Action,
    desc: &'static str,
) -> Binding {
    Binding { keys, action, desc }
}

pub const KEYMAP: &[Binding] = &[
    bind("gd", Action::Definition, "go to definition of the term or link under the cursor"),
    bind("<C-]>", Action::Definition, "go to definition"),
    bind("<CR>", Action::Follow, "follow the link under the cursor"),
    bind("gD", Action::Definitions, "list every definition candidate"),
    bind("gr", Action::References, "find references to the symbol under the cursor, or to this section"),
    bind("K", Action::Hover, "peek at the definition in a popup"),
    bind("gx", Action::OpenBrowser, "open the link or section in the browser"),
    bind("gO", Action::PickOutline, "document outline"),
    bind("gh", Action::Home, "home: the list of documents"),
    bind("<C-o>", Action::JumpBack, "jump back"),
    bind("<C-t>", Action::JumpBack, "jump back"),
    bind("<BS>", Action::JumpBack, "jump back"),
    bind("<Tab>", Action::JumpForward, "jump forward"),
    bind("<C-^>", Action::Alternate, "alternate document"),
    bind("tt", Action::TabNew, "new tab at the current position"),
    bind("td", Action::TabClose, "close tab"),
    bind("tk", Action::TabNext, "next tab"),
    bind("tn", Action::TabNext, "next tab"),
    bind("tj", Action::TabPrev, "previous tab"),
    bind("th", Action::TabFirst, "first tab"),
    bind("tl", Action::TabLast, "last tab"),
    bind("<Space>f", Action::PickSpecs, "find document"),
    bind("<Space><Space>", Action::PickSpecs, "find document"),
    bind("<Space>o", Action::PickOutline, "document outline"),
    bind("<Space>s", Action::PickSymbols, "workspace symbols: types, objects, instance declarations, NodeIds"),
    bind("<Space>t", Action::PickTerms, "defined terms"),
    bind("<Space>h", Action::PickHeadings, "headings across all documents"),
    bind("<Space>/", Action::LiveGrep, "full-text search across all documents"),
    bind("<Space>g", Action::LiveGrep, "full-text search across all documents"),
    bind("<Space>w", Action::GrepWord, "full-text search for the word under the cursor"),
    bind("<Space>r", Action::References, "find references"),
    bind("<Space>m", Action::PickMarks, "marks"),
    bind("<Space>j", Action::PickJumps, "jump list"),
    bind("<Space>e", Action::ToggleOutline, "toggle the outline sidebar"),
    bind("<Space>?", Action::PickKeymaps, "key bindings"),
    bind("h", Action::Left, "left"),
    bind("<Left>", Action::Left, "left"),
    bind("l", Action::Right, "right"),
    bind("<Right>", Action::Right, "right"),
    bind("j", Action::Down, "down"),
    bind("<Down>", Action::Down, "down"),
    bind("k", Action::Up, "up"),
    bind("<Up>", Action::Up, "up"),
    bind("w", Action::WordForward(false), "next word"),
    bind("W", Action::WordForward(true), "next WORD"),
    bind("b", Action::WordBack(false), "previous word"),
    bind("B", Action::WordBack(true), "previous WORD"),
    bind("e", Action::WordEnd(false), "end of word"),
    bind("E", Action::WordEnd(true), "end of WORD"),
    bind("0", Action::LineStart, "start of line"),
    bind("<Home>", Action::LineStart, "start of line"),
    bind("^", Action::FirstNonBlank, "first non-blank"),
    bind("$", Action::LineEnd, "end of line"),
    bind("<End>", Action::LineEnd, "end of line"),
    bind("gg", Action::Top, "first line (or line N)"),
    bind("G", Action::Bottom, "last line (or line N)"),
    bind("%", Action::Percent, "N percent into the document"),
    bind("H", Action::ScreenTop, "top of screen"),
    bind("M", Action::ScreenMiddle, "middle of screen"),
    bind("L", Action::ScreenBottom, "bottom of screen"),
    bind("}", Action::ParagraphForward, "next paragraph"),
    bind("{", Action::ParagraphBack, "previous paragraph"),
    bind("]]", Action::NextSection, "next section"),
    bind("[[", Action::PrevSection, "previous section"),
    bind("[u", Action::ParentSection, "parent section"),
    bind("]t", Action::NextTable, "next table"),
    bind("[t", Action::PrevTable, "previous table"),
    bind("]f", Action::NextFigure, "next figure"),
    bind("[f", Action::PrevFigure, "previous figure"),
    bind("]l", Action::NextLink, "next link or term"),
    bind("[l", Action::PrevLink, "previous link or term"),
    bind(";", Action::RepeatFind, "repeat f/t"),
    bind(",", Action::RepeatFindReverse, "repeat f/t backwards"),
    bind("<C-d>", Action::HalfPageDown, "half page down"),
    bind("<C-u>", Action::HalfPageUp, "half page up"),
    bind("<C-f>", Action::PageDown, "page down"),
    bind("<PageDown>", Action::PageDown, "page down"),
    bind("<C-b>", Action::PageUp, "page up"),
    bind("<PageUp>", Action::PageUp, "page up"),
    bind("<C-e>", Action::ScrollDown, "scroll down"),
    bind("<C-y>", Action::ScrollUp, "scroll up"),
    bind("zt", Action::ScrollTop, "cursor line to top"),
    bind("z<CR>", Action::ScrollTop, "cursor line to top"),
    bind("zz", Action::ScrollCenter, "cursor line to center"),
    bind("z.", Action::ScrollCenter, "cursor line to center"),
    bind("zb", Action::ScrollBottom, "cursor line to bottom"),
    bind("z-", Action::ScrollBottom, "cursor line to bottom"),
    bind("yy", Action::YankLine, "yank line"),
    bind("yu", Action::YankUrl, "yank the section's web URL"),
    bind("yc", Action::YankCitation, "yank a citation, e.g. OPC 10000-4, 5.7.2"),
    bind("yn", Action::YankNodeId, "yank the NodeId under the cursor or of this section"),
    bind("v", Action::VisualChar, "visual mode"),
    bind("V", Action::VisualLine, "visual line mode"),
    bind("/", Action::SearchForward, "search forward"),
    bind("?", Action::SearchBackward, "search backward"),
    bind("n", Action::SearchNext, "next match"),
    bind("N", Action::SearchPrev, "previous match"),
    bind("*", Action::StarForward, "search word under cursor"),
    bind("#", Action::StarBackward, "search word under cursor backwards"),
    bind("<Esc>", Action::ClearHighlight, "clear search highlight"),
    bind(":", Action::Command, "command line"),
    bind("<C-g>", Action::Info, "document info popup"),
    bind("<C-l>", Action::Redraw, "redraw"),
    bind("ZZ", Action::Quit, "quit"),
    bind("ZQ", Action::Quit, "quit"),
];

/// Prefixes that take one following character as an argument.
const ARG_PREFIXES: &[&str] = &["m", "'", "`", "f", "F", "T"];

pub fn arg_hint(prefix: &str) -> Option<&'static str> {
    Some(match prefix {
        "m" => "set mark: a-z in this document, A-Z global",
        "'" => "jump to mark (line); '' jumps back",
        "`" => "jump to mark (exact position)",
        "f" | "F" | "T" => "find character on this line",
        _ => return None,
    })
}

pub fn token(key: KeyEvent) -> Option<String> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let named = |name: &str| Some(format!("<{name}>"));
    match key.code {
        KeyCode::Char(c) if ctrl => match c {
            '5' | ']' => named("C-]"),
            '6' | '^' => named("C-^"),
            c => named(&format!("C-{}", c.to_ascii_lowercase())),
        },
        KeyCode::Char(c) if alt => named(&format!("A-{c}")),
        KeyCode::Char(' ') => named("Space"),
        KeyCode::Char(c) => Some(c.to_string()),
        KeyCode::Enter => named("CR"),
        KeyCode::Esc => named("Esc"),
        KeyCode::Tab => named("Tab"),
        KeyCode::BackTab => named("S-Tab"),
        KeyCode::Backspace => named("BS"),
        KeyCode::Up => named("Up"),
        KeyCode::Down => named("Down"),
        KeyCode::Left => named("Left"),
        KeyCode::Right => named("Right"),
        KeyCode::PageUp => named("PageUp"),
        KeyCode::PageDown => named("PageDown"),
        KeyCode::Home => named("Home"),
        KeyCode::End => named("End"),
        _ => None,
    }
}

pub enum Outcome {
    Run(Action, Option<usize>),
    Wait,
    Unmapped(String),
}

#[derive(Debug, Default)]
pub struct Pending {
    pub count: Option<usize>,
    pub keys: String,
}

impl Pending {
    pub fn feed(
        &mut self,
        token: &str,
    ) -> Outcome {
        if self.keys.is_empty()
            && let Some(d) = digit(token)
            && (d != 0 || self.count.is_some())
        {
            self.count = Some(self.count.unwrap_or(0).saturating_mul(10).saturating_add(d));
            return Outcome::Wait;
        }
        if ARG_PREFIXES.contains(&self.keys.as_str())
            && let Some(c) = single_char(token)
        {
            let action = with_arg(&self.keys, c);
            return Outcome::Run(action, self.take().0);
        }
        self.keys.push_str(token);
        if let Some(binding) = KEYMAP.iter().find(|b| b.keys == self.keys) {
            return Outcome::Run(binding.action, self.take().0);
        }
        let waiting =
            ARG_PREFIXES.contains(&self.keys.as_str()) || KEYMAP.iter().any(|b| b.keys.starts_with(&self.keys));
        if waiting {
            return Outcome::Wait;
        }
        Outcome::Unmapped(self.take().1)
    }

    pub fn take(&mut self) -> (Option<usize>, String) {
        (self.count.take(), std::mem::take(&mut self.keys))
    }

    pub fn display(&self) -> String {
        let count = self.count.map(|c| c.to_string()).unwrap_or_default();
        format!("{count}{}", self.keys)
    }

    /// Bindings that continue the keys typed so far, for the which-key popup.
    pub fn continuations(&self) -> Vec<(String, &'static str)> {
        if self.keys.is_empty() {
            return Vec::new();
        }
        if let Some(hint) = arg_hint(&self.keys) {
            return vec![("{char}".to_owned(), hint)];
        }
        let mut out: Vec<(String, &'static str)> = Vec::new();
        for b in KEYMAP
            .iter()
            .filter(|b| b.keys.starts_with(&self.keys) && b.keys != self.keys)
        {
            let rest = b.keys[self.keys.len()..].to_owned();
            if !out.iter().any(|(_, d)| *d == b.desc) {
                out.push((rest, b.desc));
            }
        }
        out
    }
}

fn digit(token: &str) -> Option<usize> {
    let c = single_char(token)?;
    c.to_digit(10).map(|d| d as usize)
}

fn single_char(token: &str) -> Option<char> {
    if token == "<Space>" {
        return Some(' ');
    }
    let mut chars = token.chars();
    let c = chars.next()?;
    (chars.next().is_none()).then_some(c)
}

fn with_arg(
    prefix: &str,
    c: char,
) -> Action {
    match prefix {
        "m" => Action::SetMark(c),
        "'" => Action::GotoMark(c, false),
        "`" => Action::GotoMark(c, true),
        "f" => Action::FindChar(Find::To, c),
        "F" => Action::FindChar(Find::BackTo, c),
        _ => Action::FindChar(Find::BackTill, c),
    }
}
