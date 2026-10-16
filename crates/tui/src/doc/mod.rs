pub mod layout;
mod table;
pub mod wrap;

use uareference_model::Link;
use unicode_width::UnicodeWidthStr;

use crate::corpus::SpecIdx;

/// Where a rendered line came from: section, slot (0 = heading, n = block n − 1) and row within the slot.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
pub struct Origin {
    pub section: u32,
    pub slot: u32,
    pub row: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Role {
    #[default]
    Body,
    Title,
    Heading(u8),
    Meta,
    Caption,
    Border,
    Header,
    Code,
    Note,
    Bullet,
    Muted,
}

pub const BOLD: u8 = 1;
pub const ITALIC: u8 = 2;
pub const UNDERLINE: u8 = 4;
pub const STRIKE: u8 = 8;
pub const MONO: u8 = 16;
pub const SUP: u8 = 32;
pub const SUB: u8 = 64;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Sty {
    pub role: Role,
    pub flags: u8,
}

impl Sty {
    pub const fn role(role: Role) -> Self {
        Self { role, flags: 0 }
    }

    pub const fn with(
        self,
        flags: u8,
    ) -> Self {
        Self {
            role: self.role,
            flags: self.flags | flags,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Span {
    pub text: String,
    pub sty: Sty,
    pub link: Option<u32>,
    pub width: u16,
}

impl Span {
    pub fn new(
        text: impl Into<String>,
        sty: Sty,
        link: Option<u32>,
    ) -> Self {
        let text = text.into();
        let width = text.width() as u16;
        Self { text, sty, link, width }
    }
}

#[derive(Debug, Clone)]
pub struct Line {
    pub spans: Vec<Span>,
    pub origin: Origin,
}

impl Line {
    pub fn text(&self) -> String {
        self.spans.iter().map(|s| s.text.as_str()).collect()
    }

    pub fn width(&self) -> usize {
        self.spans.iter().map(|s| s.width as usize).sum()
    }

    pub fn is_blank(&self) -> bool {
        self.spans.iter().all(|s| s.text.trim().is_empty())
    }
}

#[derive(Debug, Clone)]
pub enum LinkKind {
    /// An italic phrase, with the symbol it resolved to when the index was available at render time.
    Term {
        text: String,
        symbol: Option<u32>,
    },
    Link(Link),
    Spec(SpecIdx),
    Figure {
        url: String,
    },
}

#[derive(Debug, Clone)]
pub struct LinkRef {
    pub kind: LinkKind,
    pub section: u32,
    pub slot: u32,
    pub nth: u32,
}

#[derive(Debug, Default)]
pub struct Doc {
    pub lines: Vec<Line>,
    pub links: Vec<LinkRef>,
    /// Line of each rendered section's heading, by section index.
    pub headings: Vec<Option<u32>>,
}

impl Doc {
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// First line at or after `origin`, clamped to the document.
    pub fn line_of(
        &self,
        origin: Origin,
    ) -> usize {
        let i = self.lines.partition_point(|l| l.origin < origin);
        i.min(self.lines.len().saturating_sub(1))
    }

    pub fn section_at(
        &self,
        line: usize,
    ) -> u32 {
        self.lines.get(line).map_or(0, |l| l.origin.section)
    }

    pub fn heading_line(
        &self,
        section: u32,
    ) -> Option<usize> {
        self.headings
            .get(section as usize)
            .copied()
            .flatten()
            .map(|l| l as usize)
    }

    pub fn link_at(
        &self,
        line: usize,
        col: usize,
    ) -> Option<u32> {
        let mut x = 0;
        for span in &self.lines.get(line)?.spans {
            let w = span.width as usize;
            if col < x + w {
                return span.link;
            }
            x += w;
        }
        None
    }

    /// Line and column where the `nth` ref inline of a slot starts.
    pub fn find_ref(
        &self,
        section: u32,
        slot: u32,
        nth: u32,
    ) -> Option<(usize, usize)> {
        let start = self.line_of(Origin { section, slot, row: 0 });
        for (i, line) in self.lines.iter().enumerate().skip(start) {
            if (line.origin.section, line.origin.slot) != (section, slot) {
                break;
            }
            let mut x = 0;
            for span in &line.spans {
                if let Some(id) = span.link {
                    let link = &self.links[id as usize];
                    if link.section == section && link.slot == slot && link.nth == nth {
                        return Some((i, x));
                    }
                }
                x += span.width as usize;
            }
        }
        None
    }
}
