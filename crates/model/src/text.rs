use crate::ir::{
    Block,
    Inline,
};

pub const SITE: &str = "https://reference.opcfoundation.org";

pub fn plain(inlines: &[Inline]) -> String {
    let mut out = String::new();
    push_plain(&mut out, inlines);
    out
}

pub fn push_plain(
    out: &mut String,
    inlines: &[Inline],
) {
    for inline in inlines {
        match inline {
            Inline::Text(text) => out.push_str(text),
            Inline::Italic(children)
            | Inline::Bold(children)
            | Inline::Underline(children)
            | Inline::Strike(children)
            | Inline::Sup(children)
            | Inline::Sub(children)
            | Inline::Mono(children)
            | Inline::Link { content: children, .. } => push_plain(out, children),
            Inline::Image { .. } => out.push_str("[image]"),
            Inline::Break => out.push(' '),
        }
    }
}

/// Text of a block sequence with one space between blocks, for search and previews.
pub fn blocks_plain(blocks: &[Block]) -> String {
    let mut out = String::new();
    for block in blocks {
        if !out.is_empty() {
            out.push(' ');
        }
        push_block_plain(&mut out, block);
    }
    out
}

fn push_block_plain(
    out: &mut String,
    block: &Block,
) {
    match block {
        Block::Paragraph { content, .. } => push_plain(out, content),
        Block::List { items, .. } => {
            for item in items {
                out.push_str(&blocks_plain(item));
                out.push(' ');
            }
        }
        Block::Table(table) => {
            out.push_str(table.label.as_deref().unwrap_or_default());
            out.push(' ');
            push_plain(out, &table.caption);
            for row in table.head.iter().chain(&table.body) {
                for cell in &row.cells {
                    out.push(' ');
                    out.push_str(&blocks_plain(&cell.blocks));
                }
            }
        }
        Block::Figure { label, caption, .. } => {
            out.push_str(label.as_deref().unwrap_or_default());
            out.push(' ');
            push_plain(out, caption);
        }
        Block::Note { label, blocks } => {
            out.push_str(label.as_deref().unwrap_or_default());
            out.push(' ');
            out.push_str(&blocks_plain(blocks));
        }
        Block::Code { text } => out.push_str(text),
        Block::Reference(reference) => push_plain(out, &reference.citation),
    }
}

/// The online reference URL of a clause; annex labels become lower-case slugs (`B.3.9` → `b-3-9`).
pub fn web_url(
    spec: &str,
    label: Option<&str>,
) -> String {
    match label {
        Some(label) if label.starts_with(|c: char| c.is_ascii_digit()) => format!("{SITE}/specs/{spec}/{label}"),
        Some(label) if !label.contains('.') => format!("{SITE}/specs/{spec}/annex-{}", label.to_ascii_lowercase()),
        Some(label) => format!("{SITE}/specs/{spec}/{}", label.to_ascii_lowercase().replace('.', "-")),
        None => format!("{SITE}/specs/{spec}"),
    }
}

/// Canonical site id for user or citation input: `10000-4`, `opc 10000-4`, `OPC 10000-4` and
/// `IEC 62541-4` all become `OPC-10000-4`.
pub fn normalize_spec_id(input: &str) -> Option<String> {
    let upper = input.trim().to_ascii_uppercase();
    let rest = upper
        .strip_prefix("OPC")
        .unwrap_or(&upper)
        .trim_start_matches([' ', '-', '_']);
    let rest = match rest
        .strip_prefix("IEC")
        .or_else(|| upper.strip_prefix("IEC"))
    {
        Some(iec) => iec.trim().replacen("62541", "10000", 1),
        None => rest.to_owned(),
    };
    let number: String = rest
        .chars()
        .take_while(|c| c.is_ascii_digit() || matches!(c, '-' | ' ' | '_'))
        .map(|c| if c.is_ascii_digit() { c } else { '-' })
        .collect();
    let number = number.trim_matches('-');
    let starts_with_doc_number = number.chars().take_while(char::is_ascii_digit).count() == 5;
    starts_with_doc_number.then(|| format!("OPC-{number}"))
}
