use uareference_model::text::blocks_plain;
use uareference_model::{
    Block,
    Cell,
    Row,
    Table,
};

use super::layout::{
    Builder,
    code_lines,
};
use super::wrap::{
    Frag,
    Word,
    longest_word,
    merge,
    natural_width,
    words,
    wrap,
};
use super::{
    BOLD,
    Role,
    Span,
    Sty,
};

/// Narrower than this, a word is split rather than allowed to widen its column.
const MIN_CAP: usize = 24;
const MIN_COLUMN: usize = 3;

struct LaidCell {
    span: usize,
    paras: Vec<Vec<Word>>,
}

impl LaidCell {
    fn min(&self) -> usize {
        self.paras
            .iter()
            .map(|p| longest_word(p))
            .max()
            .unwrap_or(0)
            .min(MIN_CAP)
    }

    fn max(&self) -> usize {
        self.paras
            .iter()
            .map(|p| natural_width(p))
            .max()
            .unwrap_or(0)
    }

    fn lines(
        &self,
        width: usize,
    ) -> Vec<Vec<Span>> {
        self.paras.iter().flat_map(|p| wrap(p, width)).collect()
    }
}

struct LaidRow {
    header: bool,
    cells: Vec<LaidCell>,
}

impl LaidRow {
    /// Column boundaries this row's cells draw, as column indexes.
    fn edges(&self) -> Vec<usize> {
        let mut edges = vec![0];
        let mut col = 0;
        for cell in &self.cells {
            col += cell.span;
            edges.push(col);
        }
        edges
    }
}

pub(super) fn layout(
    b: &mut Builder<'_>,
    table: &Table,
    width: usize,
) -> Vec<Vec<Span>> {
    let mut out = caption(b, table, width);
    let rows = laid_rows(b, table);
    let columns = rows
        .iter()
        .map(|r| r.cells.iter().map(|c| c.span).sum::<usize>())
        .max()
        .unwrap_or(0);
    if columns == 0 {
        return out;
    }
    match column_widths(&rows, columns, width) {
        Some(widths) => out.extend(grid(&rows, &widths)),
        None => out.extend(records(&rows, width)),
    }
    out
}

fn caption(
    b: &mut Builder<'_>,
    table: &Table,
    width: usize,
) -> Vec<Vec<Span>> {
    let sty = Sty::role(Role::Caption);
    let mut frags = Vec::new();
    if let Some(label) = &table.label {
        frags.push(Frag::Text {
            text: label.clone(),
            sty: sty.with(BOLD),
            link: None,
        });
        if !table.caption.is_empty() {
            frags.push(Frag::Text {
                text: " – ".to_owned(),
                sty,
                link: None,
            });
        }
    }
    b.frags(&table.caption, sty, None, &mut frags);
    if frags.is_empty() {
        return Vec::new();
    }
    wrap(&words(&frags), width)
}

fn laid_rows(
    b: &mut Builder<'_>,
    table: &Table,
) -> Vec<LaidRow> {
    let head = table.head.iter().map(|r| laid_row(b, r, true));
    let head: Vec<LaidRow> = head.collect();
    let body: Vec<LaidRow> = table.body.iter().map(|r| laid_row(b, r, false)).collect();
    head.into_iter().chain(body).collect()
}

fn laid_row(
    b: &mut Builder<'_>,
    row: &Row,
    in_head: bool,
) -> LaidRow {
    let header = in_head || (!row.cells.is_empty() && row.cells.iter().all(|c| c.header));
    let cells = row.cells.iter().map(|c| laid_cell(b, c, header)).collect();
    LaidRow { header, cells }
}

fn laid_cell(
    b: &mut Builder<'_>,
    cell: &Cell,
    header: bool,
) -> LaidCell {
    let sty = if header || cell.header {
        Sty::role(Role::Header).with(BOLD)
    } else {
        Sty::default()
    };
    let mut paras = Vec::new();
    for block in &cell.blocks {
        cell_paras(b, block, sty, &mut paras);
    }
    LaidCell {
        span: cell.colspan.max(1) as usize,
        paras,
    }
}

fn cell_paras(
    b: &mut Builder<'_>,
    block: &Block,
    sty: Sty,
    out: &mut Vec<Vec<Word>>,
) {
    match block {
        Block::Paragraph { content, .. } => out.push(b.words(content, sty)),
        Block::List { ordered, items } => {
            for (i, item) in items.iter().enumerate() {
                let marker = if *ordered {
                    format!("{}.", i + 1)
                } else {
                    "•".to_owned()
                };
                let start = out.len();
                item.iter().for_each(|block| cell_paras(b, block, sty, out));
                match out.get_mut(start) {
                    Some(first) => first.insert(0, Word::plain(&marker, Sty::role(Role::Bullet))),
                    None => out.push(vec![Word::plain(&marker, Sty::role(Role::Bullet))]),
                }
            }
        }
        Block::Code { text } => {
            for line in code_lines(text, usize::MAX / 2) {
                let text: String = line.iter().map(|s| s.text.as_str()).collect();
                out.push(vec![Word::plain(&text, Sty::role(Role::Code))]);
            }
        }
        Block::Note { blocks, .. } => blocks
            .iter()
            .for_each(|block| cell_paras(b, block, sty, out)),
        Block::Table(nested) => {
            for row in nested.head.iter().chain(&nested.body) {
                let mut frags = Vec::new();
                for (i, cell) in row.cells.iter().enumerate() {
                    if i > 0 {
                        frags.push(Frag::Text {
                            text: " │ ".to_owned(),
                            sty: Sty::role(Role::Border),
                            link: None,
                        });
                    }
                    for block in &cell.blocks {
                        if let Block::Paragraph { content, .. } = block {
                            b.frags(content, sty, None, &mut frags);
                            frags.push(Frag::Text {
                                text: " ".to_owned(),
                                sty,
                                link: None,
                            });
                        } else {
                            frags.push(Frag::Text {
                                text: blocks_plain(std::slice::from_ref(block)),
                                sty,
                                link: None,
                            });
                        }
                    }
                }
                out.push(words(&frags));
            }
        }
        Block::Figure { label, caption, .. } => {
            let mut frags = Vec::new();
            if let Some(label) = label {
                frags.push(Frag::Text {
                    text: format!("{label} "),
                    sty: sty.with(BOLD),
                    link: None,
                });
            }
            b.frags(caption, sty, None, &mut frags);
            out.push(words(&frags));
        }
        Block::Reference(reference) => out.push(b.words(&reference.citation, sty)),
    }
}

fn column_widths(
    rows: &[LaidRow],
    columns: usize,
    width: usize,
) -> Option<Vec<usize>> {
    let budget = width.checked_sub(3 * columns + 1)?;
    if budget < MIN_COLUMN * columns {
        return None;
    }
    let (min, max) = column_bounds(rows, columns);
    let min_sum: usize = min.iter().sum();
    let max_sum: usize = max.iter().sum();
    if max_sum <= budget {
        return Some(max);
    }
    if min_sum <= budget {
        return Some(distribute(&min, &max, budget - min_sum));
    }
    Some(shrink(&min, budget))
}

fn column_bounds(
    rows: &[LaidRow],
    columns: usize,
) -> (Vec<usize>, Vec<usize>) {
    let mut min = vec![MIN_COLUMN; columns];
    let mut max = vec![MIN_COLUMN; columns];
    for spanning in [false, true] {
        for row in rows {
            let mut col = 0;
            for cell in &row.cells {
                let cols = col..(col + cell.span).min(columns);
                col += cell.span;
                if (cell.span > 1) != spanning || cols.is_empty() {
                    continue;
                }
                widen(&mut min[cols.clone()], cell.min());
                widen(&mut max[cols], cell.max());
            }
        }
    }
    for (lo, hi) in min.iter_mut().zip(&mut max) {
        *hi = (*hi).max(*lo);
    }
    (min, max)
}

/// Grows a run of columns so that, joined by separators, they are at least `need` wide.
fn widen(
    cols: &mut [usize],
    need: usize,
) {
    let have: usize = cols.iter().sum::<usize>() + 3 * (cols.len() - 1);
    if need <= have {
        return;
    }
    let extra = need - have;
    let n = cols.len();
    for (i, col) in cols.iter_mut().enumerate() {
        *col += extra / n + usize::from(i < extra % n);
    }
}

fn distribute(
    min: &[usize],
    max: &[usize],
    spare: usize,
) -> Vec<usize> {
    let want: usize = max.iter().zip(min).map(|(hi, lo)| hi - lo).sum();
    let mut widths: Vec<usize> = min
        .iter()
        .zip(max)
        .map(|(lo, hi)| lo + (hi - lo) * spare / want.max(1))
        .collect();
    let mut left = spare.saturating_sub(widths.iter().zip(min).map(|(w, lo)| w - lo).sum());
    for (w, hi) in widths.iter_mut().zip(max) {
        if left == 0 {
            break;
        }
        if *w < *hi {
            *w += 1;
            left -= 1;
        }
    }
    widths
}

fn shrink(
    min: &[usize],
    budget: usize,
) -> Vec<usize> {
    let mut widths = min.to_vec();
    let mut total: usize = widths.iter().sum();
    while total > budget {
        let Some(widest) = widths
            .iter_mut()
            .filter(|w| **w > MIN_COLUMN)
            .max_by_key(|w| **w)
        else {
            break;
        };
        *widest -= 1;
        total -= 1;
    }
    widths
}

fn grid(
    rows: &[LaidRow],
    widths: &[usize],
) -> Vec<Vec<Span>> {
    let n = widths.len();
    let full: Vec<usize> = (0..=n).collect();
    let mut out = vec![border(widths, None, Some(&rows[0].edges()), &full, false)];
    for (i, row) in rows.iter().enumerate() {
        out.extend(row_lines(row, widths));
        let below = rows.get(i + 1).map(LaidRow::edges);
        let double = row.header && rows.get(i + 1).is_some_and(|r| !r.header);
        out.push(border(widths, Some(&row.edges()), below.as_deref(), &full, double));
    }
    out
}

fn border(
    widths: &[usize],
    above: Option<&[usize]>,
    below: Option<&[usize]>,
    all: &[usize],
    double: bool,
) -> Vec<Span> {
    let n = widths.len();
    let mut text = String::new();
    for &k in all {
        let up = above.is_some_and(|e| e.contains(&k));
        let down = below.is_some_and(|e| e.contains(&k));
        text.push(junction(k == 0, k == n, up, down, double));
        if k < n {
            let fill = if double { "═" } else { "─" };
            text.push_str(&fill.repeat(widths[k] + 2));
        }
    }
    vec![Span::new(text, Sty::role(Role::Border), None)]
}

fn junction(
    left: bool,
    right: bool,
    up: bool,
    down: bool,
    double: bool,
) -> char {
    let chars = if double {
        ['╒', '╕', '╘', '╛', '╞', '╡', '╤', '╧', '╪', '═']
    } else {
        ['┌', '┐', '└', '┘', '├', '┤', '┬', '┴', '┼', '─']
    };
    let [
        top_left,
        top_right,
        bottom_left,
        bottom_right,
        tee_right,
        tee_left,
        tee_down,
        tee_up,
        cross,
        flat,
    ] = chars;
    match (left, right, up, down) {
        (true, _, false, _) => top_left,
        (_, true, false, _) => top_right,
        (true, _, true, false) => bottom_left,
        (_, true, true, false) => bottom_right,
        (true, _, true, true) => tee_right,
        (_, true, true, true) => tee_left,
        (_, _, false, true) => tee_down,
        (_, _, true, false) => tee_up,
        (_, _, true, true) => cross,
        (_, _, false, false) => flat,
    }
}

fn row_lines(
    row: &LaidRow,
    widths: &[usize],
) -> Vec<Vec<Span>> {
    let mut col = 0;
    let mut cells = Vec::new();
    for cell in &row.cells {
        let cols = col..(col + cell.span).min(widths.len());
        col += cell.span;
        let w = widths[cols.clone()].iter().sum::<usize>() + 3 * cols.len().saturating_sub(1);
        cells.push((w, cell.lines(w)));
    }
    if col < widths.len() {
        let w = widths[col..].iter().sum::<usize>() + 3 * (widths.len() - col - 1);
        cells.push((w, Vec::new()));
    }
    let height = cells
        .iter()
        .map(|(_, lines)| lines.len())
        .max()
        .unwrap_or(0)
        .max(1);
    let bar = Sty::role(Role::Border);
    (0..height)
        .map(|i| {
            let mut line = vec![Span::new("│", bar, None)];
            for (w, lines) in &cells {
                line.push(Span::new(" ", Sty::default(), None));
                let content = lines.get(i).cloned().unwrap_or_default();
                let used: usize = content.iter().map(|s| s.width as usize).sum();
                line.extend(content);
                line.push(Span::new(" ".repeat(w.saturating_sub(used) + 1), Sty::default(), None));
                line.push(Span::new("│", bar, None));
            }
            merge(line)
        })
        .collect()
}

/// Fallback for tables too wide for the screen: one record per row, each cell labelled with its column header.
fn records(
    rows: &[LaidRow],
    width: usize,
) -> Vec<Vec<Span>> {
    let headers: Vec<String> = rows
        .iter()
        .rfind(|r| r.header)
        .map(|r| {
            r.cells
                .iter()
                .flat_map(|c| {
                    let text: String = c
                        .paras
                        .iter()
                        .flatten()
                        .flat_map(|w| &w.pieces)
                        .map(|s| s.text.as_str())
                        .collect::<Vec<_>>()
                        .join(" ");
                    std::iter::repeat_n(text, c.span)
                })
                .collect()
        })
        .unwrap_or_default();
    let mut out = Vec::new();
    for row in rows.iter().filter(|r| !r.header) {
        out.push(vec![Span::new("─".repeat(width.min(40)), Sty::role(Role::Border), None)]);
        let mut col = 0;
        for cell in &row.cells {
            let mut paras = cell.paras.clone();
            if let (Some(header), Some(first)) = (headers.get(col), paras.first_mut()) {
                first.insert(0, Word::plain(&format!("{header}:"), Sty::role(Role::Header).with(BOLD)));
            }
            col += cell.span;
            for para in &paras {
                out.extend(wrap(para, width));
            }
        }
    }
    out
}
