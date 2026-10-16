mod theme;

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{
    Constraint,
    Layout,
    Rect,
};
use ratatui::style::{
    Color,
    Modifier,
    Style,
};
use ratatui::text::{
    Line,
    Span,
};
use ratatui::widgets::{
    Block,
    BorderType,
    Clear,
    Paragraph,
    Widget,
};
use unicode_width::UnicodeWidthStr;

use crate::app::motion::matches_in;
use crate::app::picker::Picker;
use crate::app::{
    App,
    Mode,
    ViewKey,
};
use crate::doc::Doc;

const SIDEBAR: u16 = 34;
const SIDEBAR_MIN_TOTAL: u16 = 110;

pub struct Areas {
    pub tabline: Rect,
    pub winbar: Rect,
    pub sidebar: Option<Rect>,
    pub doc: Rect,
    pub status: Rect,
    pub cmdline: Rect,
}

pub fn areas(
    area: Rect,
    app: &App,
) -> Areas {
    let [tabline, winbar, main, status, cmdline] = Layout::vertical([
        Constraint::Length(u16::from(app.tab_count() > 1)),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(area);
    if app.state.outline && area.width >= SIDEBAR_MIN_TOTAL {
        let [sidebar, doc] = Layout::horizontal([Constraint::Length(SIDEBAR), Constraint::Min(1)]).areas(main);
        Areas {
            tabline,
            winbar,
            sidebar: Some(sidebar),
            doc,
            status,
            cmdline,
        }
    } else {
        Areas {
            tabline,
            winbar,
            sidebar: None,
            doc: main,
            status,
            cmdline,
        }
    }
}

/// Document line and column under a screen cell, for mouse clicks.
pub fn doc_position(
    app: &App,
    x: u16,
    y: u16,
) -> Option<(usize, usize)> {
    let area = app.doc_rect;
    let inside = x >= area.x && x < area.right() && y >= area.y && y < area.bottom();
    inside.then(|| (app.view.top + (y - area.y) as usize, (x - area.x) as usize))
}

pub fn draw(
    frame: &mut Frame,
    app: &mut App,
) {
    let a = areas(frame.area(), app);
    app.doc_rect = a.doc;
    tabline(frame.buffer_mut(), a.tabline, app);
    winbar(frame.buffer_mut(), a.winbar, app);
    if let Some(sidebar) = a.sidebar {
        outline(frame.buffer_mut(), sidebar, app);
    }
    document(frame.buffer_mut(), a.doc, app);
    status(frame.buffer_mut(), a.status, app);
    let mut cursor = cursor_position(app, a.doc);
    if let Some(pos) = cmdline(frame.buffer_mut(), a.cmdline, app) {
        cursor = Some(pos);
    }
    if app.hover.is_some() {
        hover(frame, a.doc, app);
    }
    if app.picker.is_some() {
        cursor = picker(frame, app);
    }
    which_key(frame, a.doc, app);
    if let Some((x, y)) = cursor {
        frame.set_cursor_position((x, y));
    }
}

fn cursor_position(
    app: &App,
    area: Rect,
) -> Option<(u16, u16)> {
    let c = app.view.cursor;
    let row = c.line.checked_sub(app.view.top)?;
    (row < area.height as usize && c.col < area.width as usize).then(|| (area.x + c.col as u16, area.y + row as u16))
}

fn tabline(
    buf: &mut Buffer,
    area: Rect,
    app: &App,
) {
    if area.height == 0 {
        return;
    }
    let labels: Vec<String> = app
        .tab_specs()
        .iter()
        .enumerate()
        .map(|(i, spec)| format!(" {} {} ", i + 1, spec.as_deref().unwrap_or("home")))
        .collect();
    let mut start = 0;
    while start < app.tab
        && labels[start..=app.tab]
            .iter()
            .map(|l| l.width())
            .sum::<usize>()
            > area.width as usize
    {
        start += 1;
    }
    buf.set_style(area, theme::BAR);
    let mut x = area.x;
    for (i, label) in labels.iter().enumerate().skip(start) {
        let style = if i == app.tab { theme::TAB_ACTIVE } else { Style::new() };
        x = buf
            .set_stringn(x, area.y, label, area.right().saturating_sub(x) as usize, style)
            .0;
    }
}

fn winbar(
    buf: &mut Buffer,
    area: Rect,
    app: &App,
) {
    let mut spans = vec![];
    match (app.current_spec(), app.current_section()) {
        (Some(spec), Some(section)) => {
            if let Some(entry) = app.lib.entry(spec) {
                spans.push(Span::styled(
                    format!(" {} ", entry.spec.document),
                    Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                ));
                let chain = entry.breadcrumb(section);
                for (i, s) in chain.iter().enumerate() {
                    let style = if i + 1 == chain.len() {
                        Style::new().add_modifier(Modifier::BOLD)
                    } else {
                        theme::MUTED
                    };
                    spans.push(Span::styled("› ", theme::MUTED));
                    spans.push(Span::styled(format!("{} ", entry.heading(*s)), style));
                }
            }
        }
        _ => spans
            .push(Span::styled(" OPC UA Online Reference ", Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD))),
    }
    let width: usize = spans.iter().map(|s| s.content.width()).sum();
    if width > area.width as usize && spans.len() > 3 {
        let mut drop = width - area.width as usize + 2;
        let mut i = 1;
        while drop > 0 && i + 2 < spans.len() {
            drop = drop.saturating_sub(spans[i].content.width());
            spans[i] = Span::raw("");
            i += 1;
        }
        spans[1] = Span::styled("…", theme::MUTED);
    }
    Paragraph::new(Line::from(spans)).render(area, buf);
}

fn outline(
    buf: &mut Buffer,
    area: Rect,
    app: &App,
) {
    let block = Block::new()
        .borders(ratatui::widgets::Borders::RIGHT)
        .border_style(theme::MUTED);
    let inner = block.inner(area);
    block.render(area, buf);
    let (Some(spec), Some(section)) = (app.current_spec(), app.current_section()) else {
        let text = Paragraph::new(" Enter on a document opens it").style(theme::MUTED);
        text.render(inner, buf);
        return;
    };
    let Some(entry) = app.lib.entry(spec) else {
        return;
    };
    let chain = entry.breadcrumb(section);
    let rows: Vec<u32> = (0..entry.spec.sections.len() as u32)
        .filter(|&s| {
            let sec = entry.section(s);
            sec.depth == 0 || sec.parent.is_some_and(|p| chain.contains(&p))
        })
        .collect();
    let current = rows.iter().position(|&s| s == section).unwrap_or(0);
    let height = inner.height as usize;
    let start = current
        .saturating_sub(height / 2)
        .min(rows.len().saturating_sub(height));
    for (row, &s) in rows.iter().skip(start).take(height).enumerate() {
        let sec = entry.section(s);
        let indent = "  ".repeat(sec.depth as usize);
        let text = format!("{indent}{}", entry.heading(s));
        let style = if s == section {
            Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else if chain.contains(&s) {
            Style::new().add_modifier(Modifier::BOLD)
        } else if sec.depth == 0 {
            Style::new()
        } else {
            theme::MUTED
        };
        let y = inner.y + row as u16;
        buf.set_stringn(inner.x, y, truncate(&text, inner.width as usize), inner.width as usize, style);
    }
}

fn document(
    buf: &mut Buffer,
    area: Rect,
    app: &App,
) {
    let doc = app.view.doc.clone();
    let pattern = highlight_pattern(app);
    let selection = app.selection();
    let link_here = doc.link_at(app.view.cursor.line, app.view.cursor.col);
    for row in 0..area.height {
        let l = app.view.top + row as usize;
        let Some(line) = doc.lines.get(l) else {
            break;
        };
        let y = area.y + row;
        draw_line(buf, area.x, y, area.width, &doc, l, pattern.as_deref(), link_here);
        if let Some(((sl, sc), (el, ec), linewise)) = selection
            && (sl..=el).contains(&l)
        {
            let from = if linewise || l > sl { 0 } else { sc };
            let to = if linewise || l < el {
                line.width().max(1)
            } else {
                ec + 1
            };
            restyle(buf, area, y, from, to, theme::SELECTION);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_line(
    buf: &mut Buffer,
    x0: u16,
    y: u16,
    width: u16,
    doc: &Doc,
    l: usize,
    pattern: Option<&str>,
    link_here: Option<u32>,
) {
    let line = &doc.lines[l];
    let mut x = 0usize;
    for span in &line.spans {
        if x >= width as usize {
            break;
        }
        let mut style = theme::span(span, &doc.links);
        if link_here.is_some() && span.link == link_here && !span.text.trim().is_empty() {
            style = style.patch(theme::LINK_UNDER_CURSOR);
        }
        buf.set_stringn(x0 + x as u16, y, &span.text, width as usize - x, style);
        x += span.width as usize;
    }
    if let Some(pattern) = pattern {
        let area = Rect::new(x0, y, width, 1);
        for (from, to) in matches_in(&line.text(), pattern) {
            restyle(buf, area, y, from, to, theme::SEARCH);
        }
    }
}

fn restyle(
    buf: &mut Buffer,
    area: Rect,
    y: u16,
    from: usize,
    to: usize,
    style: Style,
) {
    for col in from..to.min(area.width as usize) {
        if let Some(cell) = buf.cell_mut((area.x + col as u16, y)) {
            cell.set_style(style);
        }
    }
}

fn highlight_pattern(app: &App) -> Option<String> {
    if let Some(cl) = &app.cmdline
        && cl.prompt() != ':'
        && !cl.text.is_empty()
    {
        return Some(cl.text.clone());
    }
    app.search
        .highlight
        .then(|| app.search.pattern.clone())
        .flatten()
}

fn status(
    buf: &mut Buffer,
    area: Rect,
    app: &App,
) {
    let (mode, color) = match app.mode {
        Mode::Normal => (" NORMAL ", Color::Blue),
        Mode::Visual { line: false, .. } => (" VISUAL ", Color::Magenta),
        Mode::Visual { line: true, .. } => (" V-LINE ", Color::Magenta),
    };
    let mut left = vec![Span::styled(
        mode,
        Style::new()
            .fg(Color::Black)
            .bg(color)
            .add_modifier(Modifier::BOLD),
    )];
    let mut right = Vec::new();
    match app.view.key {
        ViewKey::Spec(idx) => {
            let id = &app.lib.ids[idx as usize];
            let version = app
                .lib
                .catalog
                .get(id)
                .map(|e| e.version.as_str())
                .unwrap_or_default();
            left.push(Span::styled(format!(" {id} "), Style::new().add_modifier(Modifier::BOLD)));
            left.push(Span::styled(format!("{version} "), theme::MUTED));
            left.push(Span::raw(app.lib.title(idx).to_owned()));
        }
        ViewKey::Home => left.push(Span::raw(" home")),
    }
    if let Some(job) = &app.sync {
        right.push(Span::styled(
            format!(" sync {}/{} {} ", job.done, job.total, job.current),
            Style::new().fg(Color::Yellow),
        ));
    }
    if app.is_indexing() {
        right.push(Span::styled(" indexing… ", Style::new().fg(Color::Yellow)));
    }
    let pending = app.pending.display();
    if !pending.is_empty() {
        right.push(Span::styled(format!(" {pending} "), Style::new().fg(Color::Cyan)));
    }
    let total = app.view.doc.len().max(1);
    let line = app.view.cursor.line + 1;
    right.push(Span::styled(
        format!(" {line}/{total} {:>3}% ", line * 100 / total),
        Style::new().fg(Color::Black).bg(color),
    ));
    let right_width: usize = right.iter().map(|s| s.content.width()).sum();
    buf.set_style(area, theme::BAR);
    let left_width = (area.width as usize).saturating_sub(right_width);
    let mut x = area.x;
    for span in left {
        let rest = (area.x as usize + left_width).saturating_sub(x as usize);
        x = buf
            .set_stringn(x, area.y, &span.content, rest, span.style)
            .0;
    }
    let mut x = area.x + left_width as u16;
    for span in right {
        x = buf
            .set_stringn(x, area.y, &span.content, area.right().saturating_sub(x) as usize, span.style)
            .0;
    }
}

fn cmdline(
    buf: &mut Buffer,
    area: Rect,
    app: &App,
) -> Option<(u16, u16)> {
    if let Some(cl) = &app.cmdline {
        let text = format!("{}{}", cl.prompt(), cl.text);
        buf.set_stringn(area.x, area.y, &text, area.width as usize, Style::new());
        let col = 1 + cl.text[..cl.cursor].width();
        return Some((area.x + col.min((area.width as usize).saturating_sub(1)) as u16, area.y));
    }
    if let Some(msg) = &app.message {
        let style = if msg.error { theme::ERROR } else { Style::new() };
        buf.set_stringn(area.x, area.y, &msg.text, area.width as usize, style);
    }
    None
}

fn hover(
    frame: &mut Frame,
    doc_area: Rect,
    app: &App,
) {
    let Some(hover) = &app.hover else {
        return;
    };
    let width = (hover
        .doc
        .lines
        .iter()
        .map(|l| l.width())
        .max()
        .unwrap_or(20) as u16
        + 2)
    .min(doc_area.width);
    let height = (hover.doc.len() as u16 + 2).min(22).min(doc_area.height);
    let cursor_row = app.view.cursor.line.saturating_sub(app.view.top) as u16;
    let below = doc_area.y + cursor_row + 1;
    let y = if below + height <= doc_area.bottom() {
        below
    } else {
        (doc_area.y + cursor_row)
            .saturating_sub(height)
            .max(doc_area.y)
    };
    let x = (doc_area.x + app.view.cursor.col as u16).min(doc_area.right().saturating_sub(width));
    let area = Rect::new(x, y, width, height);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::BORDER)
        .title(Span::styled(format!(" {} ", hover.title), Style::new().add_modifier(Modifier::BOLD)))
        .title_bottom(Span::styled(" K/<CR> go · j/k scroll · <Esc> close ", theme::MUTED));
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);
    for row in 0..inner.height {
        let l = hover.scroll + row as usize;
        if l >= hover.doc.len() {
            break;
        }
        draw_line(frame.buffer_mut(), inner.x, inner.y + row, inner.width, &hover.doc, l, None, None);
    }
}

fn picker(
    frame: &mut Frame,
    app: &mut App,
) -> Option<(u16, u16)> {
    let area = frame.area();
    let width = (area.width as f32 * 0.92) as u16;
    let height = (area.height as f32 * 0.88) as u16;
    let area = Rect::new(area.x + (area.width - width) / 2, area.y + (area.height - height) / 2, width, height);
    frame.render_widget(Clear, area);
    let with_preview = width >= 100;
    let [list_area, preview_area] = if with_preview {
        Layout::horizontal([Constraint::Percentage(45), Constraint::Percentage(55)]).areas(area)
    } else {
        [area, Rect::default()]
    };
    if with_preview {
        let inner_width = preview_area.width.saturating_sub(2) as usize;
        let preview = app.preview(inner_width);
        draw_preview(frame, preview_area, preview, app.picker.as_ref().map_or(0, |p| p.preview_scroll));
    }
    let picker = app.picker.as_ref()?;
    draw_list(frame, list_area, picker)
}

fn draw_list(
    frame: &mut Frame,
    area: Rect,
    picker: &Picker,
) -> Option<(u16, u16)> {
    let count = format!(" {}/{} ", picker.filtered.len(), picker.items.len());
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::BORDER)
        .title(Span::styled(format!(" {} ", picker.title), Style::new().add_modifier(Modifier::BOLD)))
        .title_bottom(Span::styled(count, theme::MUTED));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let buf = frame.buffer_mut();
    let prompt = format!("❯ {}", picker.query);
    buf.set_stringn(inner.x, inner.y, &prompt, inner.width as usize, Style::new().fg(Color::Cyan));
    let cursor = (inner.x + prompt.width().min((inner.width as usize).saturating_sub(1)) as u16, inner.y);
    let rows = inner.height.saturating_sub(2) as usize;
    if rows == 0 {
        return Some(cursor);
    }
    buf.set_stringn(inner.x, inner.y + 1, "─".repeat(inner.width as usize), inner.width as usize, theme::MUTED);
    let start = picker.selected.saturating_sub(rows - 1);
    for (row, &i) in picker.filtered.iter().skip(start).take(rows).enumerate() {
        let item = &picker.items[i as usize];
        let y = inner.y + 2 + row as u16;
        let selected = start + row == picker.selected;
        let base = if selected { theme::PICK_SELECTED } else { Style::new() };
        if selected {
            buf.set_style(Rect::new(inner.x, y, inner.width, 1), base);
        }
        let marker = if selected { "▶ " } else { "  " };
        let label = format!("{marker}{}", item.label);
        let (x, _) = buf.set_stringn(inner.x, y, &label, inner.width as usize, base);
        let rest = (inner.right()).saturating_sub(x + 2) as usize;
        if rest > 0 && !item.detail.is_empty() {
            buf.set_stringn(x + 2, y, &item.detail, rest, base.patch(theme::MUTED));
        }
    }
    Some(cursor)
}

fn draw_preview(
    frame: &mut Frame,
    area: Rect,
    preview: Option<(std::sync::Arc<Doc>, usize, Option<String>)>,
    scroll: isize,
) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::BORDER)
        .title(" Preview ");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let Some((doc, target, highlight)) = preview else {
        return;
    };
    let base = target.saturating_sub(2) as isize + scroll;
    let start = base.clamp(0, doc.len().saturating_sub(1) as isize) as usize;
    let buf = frame.buffer_mut();
    for row in 0..inner.height {
        let l = start + row as usize;
        if l >= doc.len() {
            break;
        }
        let y = inner.y + row;
        draw_line(buf, inner.x, y, inner.width, &doc, l, highlight.as_deref(), None);
        if l == target {
            for x in inner.x..inner.right() {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_bg(theme::TARGET_LINE.bg.unwrap_or(Color::Reset));
                }
            }
        }
    }
}

fn which_key(
    frame: &mut Frame,
    doc_area: Rect,
    app: &App,
) {
    let hints = app.pending.continuations();
    if hints.is_empty() || app.picker.is_some() {
        return;
    }
    let key_width = hints.iter().map(|(k, _)| k.width()).max().unwrap_or(1);
    let lines: Vec<Line> = hints
        .iter()
        .map(|(k, d)| {
            Line::from(vec![
                Span::styled(format!("{k:>key_width$}  "), Style::new().fg(Color::Cyan)),
                Span::raw(*d),
            ])
        })
        .collect();
    let width = (lines.iter().map(Line::width).max().unwrap_or(10) as u16 + 4).min(doc_area.width);
    let height = (lines.len() as u16 + 2).min(doc_area.height);
    let area =
        Rect::new(doc_area.right().saturating_sub(width), doc_area.bottom().saturating_sub(height), width, height);
    frame.render_widget(Clear, area);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::BORDER)
        .title(Span::styled(format!(" {} ", app.pending.display()), Style::new().fg(Color::Cyan)));
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn truncate(
    text: &str,
    width: usize,
) -> String {
    if text.width() <= width {
        return text.to_owned();
    }
    let mut out = String::new();
    let mut used = 0;
    for c in text.chars() {
        let w = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if used + w + 1 > width {
            break;
        }
        out.push(c);
        used += w;
    }
    out.push('…');
    out
}
