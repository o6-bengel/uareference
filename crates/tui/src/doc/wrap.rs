use unicode_width::{
    UnicodeWidthChar,
    UnicodeWidthStr,
};

use super::{
    Span,
    Sty,
};

/// A run of text with one style, as produced from the inline tree before wrapping.
#[derive(Debug, Clone)]
pub enum Frag {
    Text { text: String, sty: Sty, link: Option<u32> },
    Break,
}

#[derive(Debug, Clone, Default)]
pub struct Word {
    pub pieces: Vec<Span>,
    pub width: usize,
    pub break_before: bool,
}

impl Word {
    pub fn plain(
        text: &str,
        sty: Sty,
    ) -> Self {
        Self {
            pieces: vec![Span::new(text, sty, None)],
            width: text.width(),
            break_before: false,
        }
    }

    fn push(
        &mut self,
        ch: char,
        sty: Sty,
        link: Option<u32>,
    ) {
        let w = ch.width().unwrap_or(0);
        self.width += w;
        match self.pieces.last_mut() {
            Some(last) if last.sty == sty && last.link == link => {
                last.text.push(ch);
                last.width += w as u16;
            }
            _ => self.pieces.push(Span::new(ch.to_string(), sty, link)),
        }
    }
}

pub fn words(frags: &[Frag]) -> Vec<Word> {
    let mut out = Vec::new();
    let mut word = Word::default();
    let mut pending_break = false;
    for frag in frags {
        match frag {
            Frag::Break => {
                finish(&mut out, &mut word, &mut pending_break);
                pending_break = true;
            }
            Frag::Text { text, sty, link } => {
                for ch in text.chars() {
                    if ch.is_whitespace() {
                        finish(&mut out, &mut word, &mut pending_break);
                    } else {
                        word.push(ch, *sty, *link);
                    }
                }
            }
        }
    }
    finish(&mut out, &mut word, &mut pending_break);
    out
}

fn finish(
    out: &mut Vec<Word>,
    word: &mut Word,
    pending_break: &mut bool,
) {
    if word.pieces.is_empty() {
        return;
    }
    let mut done = std::mem::take(word);
    done.break_before = std::mem::take(pending_break);
    out.push(done);
}

/// Greedy wrap into lines of at most `width` columns; overlong words are split.
pub fn wrap(
    words: &[Word],
    width: usize,
) -> Vec<Vec<Span>> {
    let width = width.max(1);
    let mut lines: Vec<Vec<Span>> = Vec::new();
    let mut line: Vec<Span> = Vec::new();
    let mut used = 0;
    for word in words {
        let fits = used + 1 + word.width <= width;
        if !line.is_empty() && (word.break_before || !fits) {
            lines.push(std::mem::take(&mut line));
            used = 0;
        }
        if word.width > width {
            for chunk in split_word(word, width) {
                if !line.is_empty() {
                    lines.push(std::mem::take(&mut line));
                }
                used = chunk.iter().map(|s| s.width as usize).sum();
                line = chunk;
            }
            continue;
        }
        if !line.is_empty() {
            line.push(gap(line.last(), word.pieces.first()));
            used += 1;
        }
        line.extend(word.pieces.iter().cloned());
        used += word.width;
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines.into_iter().map(merge).collect()
}

/// The space between two words keeps a link's styling when both sides belong to the same link.
fn gap(
    before: Option<&Span>,
    after: Option<&Span>,
) -> Span {
    match (before, after) {
        (Some(b), Some(a)) if b.link.is_some() && b.link == a.link => Span::new(" ", b.sty, b.link),
        _ => Span::new(" ", Sty::default(), None),
    }
}

fn split_word(
    word: &Word,
    width: usize,
) -> Vec<Vec<Span>> {
    let mut chunks: Vec<Vec<Span>> = vec![Vec::new()];
    let mut used = 0;
    for piece in &word.pieces {
        for ch in piece.text.chars() {
            let w = ch.width().unwrap_or(0);
            if used + w > width && used > 0 {
                chunks.push(Vec::new());
                used = 0;
            }
            let chunk = chunks.last_mut().expect("chunks is never empty");
            match chunk.last_mut() {
                Some(last) if last.sty == piece.sty && last.link == piece.link => {
                    last.text.push(ch);
                    last.width += w as u16;
                }
                _ => chunk.push(Span::new(ch.to_string(), piece.sty, piece.link)),
            }
            used += w;
        }
    }
    chunks
}

pub fn merge(spans: Vec<Span>) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::with_capacity(spans.len());
    for span in spans {
        match out.last_mut() {
            Some(last) if last.sty == span.sty && last.link == span.link => {
                last.text.push_str(&span.text);
                last.width += span.width;
            }
            _ => out.push(span),
        }
    }
    out
}

/// Natural single-line width of a paragraph.
pub fn natural_width(words: &[Word]) -> usize {
    let mut widest = 0;
    let mut current = 0;
    for word in words {
        if word.break_before {
            widest = widest.max(current);
            current = 0;
        }
        current += word.width + usize::from(current > 0);
    }
    widest.max(current)
}

pub fn longest_word(words: &[Word]) -> usize {
    words.iter().map(|w| w.width).max().unwrap_or(0)
}
