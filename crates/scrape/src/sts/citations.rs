use std::collections::HashMap;

use uareference_model::text::normalize_spec_id;

use crate::dom::Element;

/// What an `xref ref-type="other"` points at: a reference entry, possibly naming an OPC document.
#[derive(Debug, Default)]
pub struct Citations {
    entries: HashMap<String, Citation>,
}

#[derive(Debug, Clone)]
pub struct Citation {
    pub spec: Option<String>,
    pub url: Option<String>,
}

impl Citations {
    pub fn collect(root: &Element) -> Self {
        let mut entries = HashMap::new();
        root.walk(&mut |element| {
            if let Some((id, citation)) = entry(element) {
                entries.entry(id).or_insert(citation);
            }
        });
        Self { entries }
    }

    pub fn get(
        &self,
        id: &str,
    ) -> Option<&Citation> {
        self.entries.get(id)
    }
}

fn entry(element: &Element) -> Option<(String, Citation)> {
    let id = element.attr("id")?;
    let is_entry = element.name == "ref" || (element.name == "p" && id.starts_with("ref_"));
    is_entry.then(|| (id.to_owned(), citation_of(element)))
}

pub fn citation_of(element: &Element) -> Citation {
    let text = element
        .child("mixed-citation")
        .map_or_else(|| element.text(), Element::text);
    Citation {
        spec: spec_of_citation(&text),
        url: url_of(element),
    }
}

fn url_of(element: &Element) -> Option<String> {
    let mut url = None;
    element.walk(&mut |e| {
        if url.is_none() && matches!(e.name.as_str(), "uri" | "ext-link") {
            url = e.attr("xlink:href").map(str::to_owned);
        }
    });
    url
}

/// The site id of the OPC document a citation starts with: `OPC 10000-6, …`, `[2] OPC 40001-1 …`,
/// `IEC 62541-3: …`, `10000-1: OPC UA Specification …`.
pub fn spec_of_citation(text: &str) -> Option<String> {
    let text = text
        .replace(['\u{2010}', '\u{2011}', '\u{2013}'], "-")
        .replace('\u{a0}', " ");
    let text = strip_bracket_labels(&text);
    let upper = text.to_ascii_uppercase();
    let rest = if let Some(rest) = upper.strip_prefix("OPC") {
        rest.trim_start_matches([' ', '-'])
    } else if let Some(rest) = upper.strip_prefix("IEC 62541") {
        return document_number(&format!("10000{rest}"));
    } else {
        upper.as_str()
    };
    document_number(rest)
}

fn document_number(rest: &str) -> Option<String> {
    let number: String = rest
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '-')
        .collect();
    let number = number.trim_end_matches('-');
    let has_doc_number = number.len() >= 5 && number.chars().take(5).all(|c| c.is_ascii_digit());
    let well_formed = number.chars().nth(5).is_none_or(|c| c == '-');
    (has_doc_number && well_formed).then(|| normalize_spec_id(&format!("OPC {number}")))?
}

fn strip_bracket_labels(text: &str) -> &str {
    let mut text = text.trim_start();
    loop {
        let Some(inner) = text.strip_prefix('[') else {
            return text;
        };
        match inner.find(']') {
            Some(end) if inner[..end].chars().all(|c| c.is_ascii_digit()) => text = inner[end + 1..].trim_start(),
            _ => return inner.trim_start(),
        }
    }
}

/// A clause label in the text right after a citation: `, 5.2.3`, `, Annex D`, `, clause 7.1`, `, A.2`.
pub fn section_after(text: &str) -> Option<String> {
    let rest = text.trim_start().strip_prefix(',')?.trim_start();
    let (keyword, rest) = split_keyword(rest);
    let token: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '.')
        .collect();
    let token = token.trim_end_matches('.');
    is_clause_label(token, keyword.is_some()).then(|| token.to_owned())
}

fn split_keyword(text: &str) -> (Option<&str>, &str) {
    for keyword in ["clause", "section", "chapter", "annex"] {
        let Some(head) = text.get(..keyword.len()) else {
            continue;
        };
        if head.eq_ignore_ascii_case(keyword) && text[keyword.len()..].starts_with(' ') {
            return (Some(keyword), text[keyword.len()..].trim_start());
        }
    }
    (None, text)
}

fn is_clause_label(
    token: &str,
    has_keyword: bool,
) -> bool {
    let mut parts = token.split('.');
    let Some(first) = parts.next() else {
        return false;
    };
    let rest: Vec<&str> = parts.collect();
    let numeric_rest = rest
        .iter()
        .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()));
    let first_numeric = !first.is_empty() && first.chars().all(|c| c.is_ascii_digit());
    let first_annex = first.len() == 1 && first.chars().all(|c| c.is_ascii_uppercase());
    numeric_rest && (first_numeric || first_annex) && (has_keyword || !rest.is_empty())
}
