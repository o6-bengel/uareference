use ratatui::style::{
    Color,
    Modifier,
    Style,
};
use uareference_model::Link;

use crate::doc::{
    BOLD,
    ITALIC,
    LinkKind,
    LinkRef,
    MONO,
    Role,
    STRIKE,
    SUB,
    SUP,
    Span,
    UNDERLINE,
};

const GRAY: Color = Color::Indexed(244);

pub const MUTED: Style = Style::new().fg(GRAY);
pub const SEARCH: Style = Style::new().fg(Color::Black).bg(Color::Yellow);
pub const SELECTION: Style = Style::new().add_modifier(Modifier::REVERSED);
pub const LINK_UNDER_CURSOR: Style = Style::new()
    .add_modifier(Modifier::BOLD)
    .add_modifier(Modifier::UNDERLINED);
pub const TARGET_LINE: Style = Style::new().bg(Color::Indexed(237));
pub const PICK_SELECTED: Style = Style::new()
    .bg(Color::Indexed(238))
    .add_modifier(Modifier::BOLD);
pub const ERROR: Style = Style::new().fg(Color::Red);
pub const BORDER: Style = Style::new().fg(Color::Blue);
pub const BAR: Style = Style::new().bg(Color::Indexed(236));
pub const TAB_ACTIVE: Style = Style::new()
    .fg(Color::Black)
    .bg(Color::Blue)
    .add_modifier(Modifier::BOLD);

pub fn role(role: Role) -> Style {
    match role {
        Role::Body => Style::new(),
        Role::Title => Style::new().fg(Color::Cyan),
        Role::Heading(0) => Style::new().fg(Color::Magenta),
        Role::Heading(1) => Style::new().fg(Color::Blue),
        Role::Heading(2) => Style::new().fg(Color::Cyan),
        Role::Heading(_) => Style::new().fg(Color::Green),
        Role::Meta => Style::new().fg(GRAY).add_modifier(Modifier::ITALIC),
        Role::Caption => Style::new().fg(Color::Green),
        Role::Border => Style::new().fg(GRAY),
        Role::Header => Style::new(),
        Role::Code => Style::new().fg(Color::Green),
        Role::Note => Style::new().fg(GRAY),
        Role::Bullet => Style::new().fg(Color::Yellow),
        Role::Muted => MUTED,
    }
}

pub fn span(
    span: &Span,
    links: &[LinkRef],
) -> Style {
    let mut style = role(span.sty.role);
    let flags = span.sty.flags;
    for (flag, modifier) in [
        (BOLD, Modifier::BOLD),
        (ITALIC, Modifier::ITALIC),
        (UNDERLINE, Modifier::UNDERLINED),
        (STRIKE, Modifier::CROSSED_OUT),
        (SUP, Modifier::DIM),
        (SUB, Modifier::DIM),
    ] {
        if flags & flag != 0 {
            style = style.add_modifier(modifier);
        }
    }
    if flags & MONO != 0 {
        style = style.fg(Color::Green);
    }
    match span
        .link
        .and_then(|id| links.get(id as usize))
        .map(|l| &l.kind)
    {
        Some(LinkKind::Term { symbol: Some(_), .. }) => style.fg(Color::Yellow),
        Some(LinkKind::Term { symbol: None, .. }) | None | Some(LinkKind::Spec(_)) => style,
        Some(LinkKind::Link(Link::Url { .. })) => style.fg(Color::Blue).add_modifier(Modifier::UNDERLINED),
        Some(LinkKind::Link(_)) => style.fg(Color::Cyan).add_modifier(Modifier::UNDERLINED),
        Some(LinkKind::Figure { .. }) => style.add_modifier(Modifier::UNDERLINED),
    }
}
