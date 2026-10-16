use std::sync::{
    Arc,
    mpsc,
};
use std::thread;

use rayon::prelude::*;
use uareference_model::text::blocks_plain;

use crate::corpus::{
    Library,
    Loc,
    SpecEntry,
    SpecIdx,
};

const MAX_HITS: usize = 2000;

#[derive(Debug, Clone, Copy)]
struct Entry {
    loc: Loc,
    start: usize,
    end: usize,
}

/// Plain text of every heading and block, ASCII-lowercased so offsets match the original text.
pub struct SearchText {
    text: String,
    entries: Vec<Entry>,
}

#[derive(Debug, Clone, Copy)]
pub struct Hit {
    pub loc: Loc,
    pub offset: usize,
}

impl SearchText {
    pub fn build(lib: &Library) -> Self {
        let parts: Vec<(String, Vec<Entry>)> = lib
            .entries
            .par_iter()
            .enumerate()
            .filter_map(|(i, e)| Some(spec_text(i as SpecIdx, e.as_deref()?)))
            .collect();
        let mut text = String::new();
        let mut entries = Vec::new();
        for (part, part_entries) in parts {
            let base = text.len();
            text.push_str(&part);
            entries.extend(part_entries.into_iter().map(|e| Entry {
                start: e.start + base,
                end: e.end + base,
                ..e
            }));
        }
        Self { text, entries }
    }

    pub fn search(
        &self,
        query: &str,
    ) -> Vec<Hit> {
        let query = query.trim().to_ascii_lowercase();
        let mut terms: Vec<&str> = query.split_whitespace().collect();
        terms.sort_by_key(|t| std::cmp::Reverse(t.len()));
        let Some(&anchor) = terms.first() else {
            return Vec::new();
        };
        let mut hits = Vec::new();
        let mut last_entry = usize::MAX;
        for (pos, _) in self.text.match_indices(anchor) {
            let e = self.entries.partition_point(|e| e.end <= pos);
            if e == last_entry || e >= self.entries.len() {
                continue;
            }
            last_entry = e;
            let entry = self.entries[e];
            let body = &self.text[entry.start..entry.end];
            if !terms.iter().all(|t| body.contains(t)) {
                continue;
            }
            let phrase = body.find(&query);
            hits.push((
                phrase.is_none(),
                entry.loc.slot != 0,
                e,
                Hit {
                    loc: entry.loc,
                    offset: phrase.unwrap_or(pos - entry.start),
                },
            ));
            if hits.len() >= MAX_HITS * 4 {
                break;
            }
        }
        hits.sort_by_key(|&(no_phrase, not_heading, e, _)| (no_phrase, not_heading, e));
        hits.into_iter()
            .take(MAX_HITS)
            .map(|(.., hit)| hit)
            .collect()
    }
}

fn spec_text(
    idx: SpecIdx,
    entry: &SpecEntry,
) -> (String, Vec<Entry>) {
    let mut text = String::new();
    let mut entries = Vec::new();
    for (s, section) in entry.spec.sections.iter().enumerate() {
        let mut push = |slot: u32, body: String| {
            let start = text.len();
            text.push_str(&body.to_ascii_lowercase());
            entries.push(Entry {
                loc: Loc {
                    spec: idx,
                    section: s as u32,
                    slot,
                },
                start,
                end: text.len(),
            });
            text.push('\n');
        };
        push(0, entry.heading(s as u32));
        for (b, block) in section.blocks.iter().enumerate() {
            push(b as u32 + 1, blocks_plain(std::slice::from_ref(block)));
        }
    }
    (text, entries)
}

/// Original-case text of the slot a hit points into.
pub fn slot_text(
    entry: &SpecEntry,
    loc: Loc,
) -> String {
    let section = entry.section(loc.section);
    match loc.slot {
        0 => entry.heading(loc.section),
        n => section
            .blocks
            .get(n as usize - 1)
            .map(|b| blocks_plain(std::slice::from_ref(b)))
            .unwrap_or_default(),
    }
}

/// A window of `text` around byte `offset`, on char boundaries.
pub fn snippet(
    text: &str,
    offset: usize,
    before: usize,
    total: usize,
) -> String {
    let mut start = offset.saturating_sub(before).min(text.len());
    while !text.is_char_boundary(start) {
        start -= 1;
    }
    let snippet: String = text[start..].chars().take(total).collect();
    let prefix = if start > 0 { "…" } else { "" };
    format!("{prefix}{}", snippet.replace('\n', " "))
}

/// Runs searches off the UI thread; only the newest query's answer matters.
pub struct Grep {
    queries: mpsc::Sender<(u64, String)>,
    answers: mpsc::Receiver<(u64, Vec<Hit>)>,
    generation: u64,
}

impl Grep {
    pub fn spawn(text: Arc<SearchText>) -> Self {
        let (queries, query_rx) = mpsc::channel::<(u64, String)>();
        let (answer_tx, answers) = mpsc::channel();
        thread::spawn(move || {
            while let Ok(mut job) = query_rx.recv() {
                while let Ok(newer) = query_rx.try_recv() {
                    job = newer;
                }
                if answer_tx.send((job.0, text.search(&job.1))).is_err() {
                    break;
                }
            }
        });
        Self {
            queries,
            answers,
            generation: 0,
        }
    }

    pub fn submit(
        &mut self,
        query: &str,
    ) -> u64 {
        self.generation += 1;
        let _ = self.queries.send((self.generation, query.to_owned()));
        self.generation
    }

    pub fn poll(&self) -> Option<(u64, Vec<Hit>)> {
        let mut latest = None;
        while let Ok(answer) = self.answers.try_recv() {
            latest = Some(answer);
        }
        latest
    }
}
