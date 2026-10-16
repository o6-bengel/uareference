use crate::doc::Doc;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Cursor {
    pub line: usize,
    pub col: usize,
    /// Column vertical motions try to return to, as in Vim's `curswant`.
    pub want: usize,
}

pub fn chars(
    doc: &Doc,
    line: usize,
) -> Vec<(char, usize)> {
    let mut out = Vec::new();
    let mut col = 0;
    let Some(line) = doc.lines.get(line) else {
        return out;
    };
    for span in &line.spans {
        for ch in span.text.chars() {
            out.push((ch, col));
            col += unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        }
    }
    out
}

fn index_at(
    cs: &[(char, usize)],
    col: usize,
) -> usize {
    cs.iter().rposition(|&(_, c)| c <= col).unwrap_or(0)
}

/// 0 for blanks and table/box decoration, 1 for word characters, 2 for punctuation.
fn class(
    c: char,
    big: bool,
) -> u8 {
    if c.is_whitespace() || matches!(c, '\u{2500}'..='\u{257F}' | '•' | '▣') {
        0
    } else if big || c.is_alphanumeric() || c == '_' {
        1
    } else {
        2
    }
}

pub fn last_col(
    doc: &Doc,
    line: usize,
) -> usize {
    chars(doc, line).last().map_or(0, |&(_, c)| c)
}

pub fn clamp_col(
    doc: &Doc,
    line: usize,
    col: usize,
) -> usize {
    let cs = chars(doc, line);
    cs.get(index_at(&cs, col)).map_or(0, |&(_, c)| c)
}

pub fn first_non_blank(
    doc: &Doc,
    line: usize,
) -> usize {
    let cs = chars(doc, line);
    cs.iter()
        .find(|&&(c, _)| class(c, false) != 0)
        .or(cs.last())
        .map_or(0, |&(_, c)| c)
}

fn is_empty_line(cs: &[(char, usize)]) -> bool {
    cs.iter().all(|&(c, _)| c.is_whitespace())
}

pub fn word_forward(
    doc: &Doc,
    line: usize,
    col: usize,
    big: bool,
) -> (usize, usize) {
    let mut l = line;
    let mut cs = chars(doc, l);
    let mut i = index_at(&cs, col);
    if let Some(&(c, _)) = cs.get(i) {
        let k = class(c, big);
        if k != 0 {
            while i < cs.len() && class(cs[i].0, big) == k {
                i += 1;
            }
        }
    }
    loop {
        while i < cs.len() && class(cs[i].0, big) == 0 {
            i += 1;
        }
        if let Some(&(_, c)) = cs.get(i) {
            return (l, c);
        }
        if l + 1 >= doc.len() {
            return (l, cs.last().map_or(0, |&(_, c)| c));
        }
        l += 1;
        cs = chars(doc, l);
        i = 0;
        if is_empty_line(&cs) {
            return (l, 0);
        }
    }
}

pub fn word_end(
    doc: &Doc,
    line: usize,
    col: usize,
    big: bool,
) -> (usize, usize) {
    let mut l = line;
    let mut cs = chars(doc, l);
    let mut i = index_at(&cs, col) + 1;
    loop {
        while i < cs.len() && class(cs[i].0, big) == 0 {
            i += 1;
        }
        if i < cs.len() {
            break;
        }
        if l + 1 >= doc.len() {
            return (l, cs.last().map_or(0, |&(_, c)| c));
        }
        l += 1;
        cs = chars(doc, l);
        i = 0;
    }
    let k = class(cs[i].0, big);
    while i + 1 < cs.len() && class(cs[i + 1].0, big) == k {
        i += 1;
    }
    (l, cs[i].1)
}

pub fn word_back(
    doc: &Doc,
    line: usize,
    col: usize,
    big: bool,
) -> (usize, usize) {
    let mut l = line;
    let mut cs = chars(doc, l);
    let mut i = index_at(&cs, col) as isize - 1;
    loop {
        while i >= 0 && class(cs[i as usize].0, big) == 0 {
            i -= 1;
        }
        if i >= 0 {
            break;
        }
        if l == 0 {
            return (0, 0);
        }
        l -= 1;
        cs = chars(doc, l);
        if is_empty_line(&cs) {
            return (l, 0);
        }
        i = cs.len() as isize - 1;
    }
    let mut i = i as usize;
    let k = class(cs[i].0, big);
    while i > 0 && class(cs[i - 1].0, big) == k {
        i -= 1;
    }
    (l, cs[i].1)
}

/// The word the cursor is on, if it is on a word character.
pub fn word_under(
    doc: &Doc,
    line: usize,
    col: usize,
) -> Option<String> {
    let cs = chars(doc, line);
    let on_word = cs
        .get(index_at(&cs, col))
        .is_some_and(|&(c, _)| class(c, false) == 1);
    on_word.then(|| word_at(doc, line, col)).flatten()
}

/// The word under the cursor (word characters only), for `*` and `#`.
pub fn word_at(
    doc: &Doc,
    line: usize,
    col: usize,
) -> Option<String> {
    let cs = chars(doc, line);
    let mut i = index_at(&cs, col);
    while i < cs.len() && class(cs[i].0, false) != 1 {
        i += 1;
    }
    if i >= cs.len() {
        return None;
    }
    let mut start = i;
    while start > 0 && class(cs[start - 1].0, false) == 1 {
        start -= 1;
    }
    let mut end = i;
    while end < cs.len() && class(cs[end].0, false) == 1 {
        end += 1;
    }
    Some(cs[start..end].iter().map(|&(c, _)| c).collect())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Find {
    To,
    Till,
    BackTo,
    BackTill,
}

impl Find {
    pub fn reversed(self) -> Self {
        match self {
            Find::To => Find::BackTo,
            Find::Till => Find::BackTill,
            Find::BackTo => Find::To,
            Find::BackTill => Find::Till,
        }
    }
}

pub fn find_char(
    doc: &Doc,
    line: usize,
    col: usize,
    find: Find,
    target: char,
    count: usize,
) -> Option<usize> {
    let cs = chars(doc, line);
    let mut i = index_at(&cs, col);
    let forward = matches!(find, Find::To | Find::Till);
    let till = matches!(find, Find::Till | Find::BackTill);
    for n in 0..count {
        let skip = usize::from(till && n == 0);
        let next = if forward {
            cs.iter()
                .enumerate()
                .skip(i + 1 + skip)
                .find(|(_, (c, _))| *c == target)
                .map(|(j, _)| j)
        } else {
            cs.iter()
                .enumerate()
                .take(i.saturating_sub(skip))
                .rev()
                .find(|(_, (c, _))| *c == target)
                .map(|(j, _)| j)
        };
        i = next?;
    }
    let i = match (till, forward) {
        (true, true) => i - 1,
        (true, false) => i + 1,
        _ => i,
    };
    Some(cs[i].1)
}

pub fn paragraph_forward(
    doc: &Doc,
    line: usize,
) -> usize {
    let mut l = line + 1;
    while l < doc.len() && doc.lines[l].is_blank() {
        l += 1;
    }
    while l < doc.len() && !doc.lines[l].is_blank() {
        l += 1;
    }
    l.min(doc.len().saturating_sub(1))
}

pub fn paragraph_back(
    doc: &Doc,
    line: usize,
) -> usize {
    let mut l = line.saturating_sub(1);
    while l > 0 && doc.lines[l].is_blank() {
        l -= 1;
    }
    while l > 0 && !doc.lines[l].is_blank() {
        l -= 1;
    }
    l
}

/// Start column of the next (or previous) link run on or after the cursor, scanning across lines.
pub fn next_link(
    doc: &Doc,
    line: usize,
    col: usize,
    forward: bool,
) -> Option<(usize, usize)> {
    let starts = |l: usize| -> Vec<usize> {
        let mut out = Vec::new();
        let mut x = 0;
        let mut prev = None;
        for span in &doc.lines[l].spans {
            if span.link.is_some() && span.link != prev && !span.text.trim().is_empty() {
                out.push(x);
            }
            prev = span.link;
            x += span.width as usize;
        }
        out
    };
    if forward {
        if let Some(&c) = starts(line).iter().find(|&&c| c > col) {
            return Some((line, c));
        }
        (line + 1..doc.len()).find_map(|l| starts(l).first().map(|&c| (l, c)))
    } else {
        if let Some(&c) = starts(line).iter().rev().find(|&&c| c < col) {
            return Some((line, c));
        }
        (0..line)
            .rev()
            .find_map(|l| starts(l).last().map(|&c| (l, c)))
    }
}

/// Byte ranges of `pattern` in a line, as display columns (start, end).
pub fn matches_in(
    text: &str,
    pattern: &str,
) -> Vec<(usize, usize)> {
    let (pattern, whole) = match pattern
        .strip_prefix("\\<")
        .and_then(|p| p.strip_suffix("\\>"))
    {
        Some(word) => (word, true),
        None => (pattern, false),
    };
    if pattern.is_empty() {
        return Vec::new();
    }
    let ignore_case = !pattern.chars().any(char::is_uppercase);
    let (hay, needle) = if ignore_case {
        (text.to_ascii_lowercase(), pattern.to_ascii_lowercase())
    } else {
        (text.to_owned(), pattern.to_owned())
    };
    let is_word = |c: Option<char>| c.is_some_and(|c| class(c, false) == 1);
    let bounded =
        |b: usize, len: usize| !is_word(text[..b].chars().next_back()) && !is_word(text[b + len..].chars().next());
    let col_of = |byte: usize| unicode_width::UnicodeWidthStr::width(&text[..byte]);
    hay.match_indices(&needle)
        .filter(|&(b, m)| !whole || bounded(b, m.len()))
        .map(|(b, m)| (col_of(b), col_of(b + m.len())))
        .collect()
}
