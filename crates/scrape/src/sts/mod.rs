mod blocks;
mod citations;
mod inlines;
mod meta;
mod sections;

use uareference_model::{
    Section,
    Spec,
};

use self::citations::Citations;
use crate::dom;
use crate::dom::Element;
use crate::error::ScrapeError;

/// Converts one NISO STS document into the IR.
pub fn parse_sts(
    id: &str,
    xml: &[u8],
) -> Result<Spec, ScrapeError> {
    let root = dom::parse(xml)?;
    let citations = Citations::collect(&root);
    let mut builder = Builder::new(&citations);
    if let Some(front) = root.child("front") {
        builder.front(front);
    }
    if let Some(body) = root.child("body") {
        builder.body(body);
    }
    if let Some(back) = root.child("back") {
        builder.back(back);
    }
    let iso_meta = root.child("front").and_then(|f| f.child("iso-meta"));
    let text_of = |name: &str| {
        iso_meta
            .and_then(|m| m.child(name))
            .map(|e| e.text().trim().to_owned())
    };
    Ok(Spec {
        id: id.to_owned(),
        document: text_of("std-ident")
            .or_else(|| text_of("doc-ident"))
            .unwrap_or_else(|| id.replacen('-', " ", 1)),
        title: iso_meta.and_then(title).unwrap_or_default(),
        version: text_of("release-version").unwrap_or_default(),
        published: iso_meta.and_then(published),
        namespaces: iso_meta.map(namespaces).unwrap_or_default(),
        sections: builder.sections,
    })
}

pub struct Builder<'a> {
    citations: &'a Citations,
    in_norm_refs: bool,
    next_auto_id: usize,
    sections: Vec<Section>,
}

impl<'a> Builder<'a> {
    fn new(citations: &'a Citations) -> Self {
        Self {
            citations,
            in_norm_refs: false,
            next_auto_id: 0,
            sections: Vec::new(),
        }
    }
}

fn title(iso_meta: &Element) -> Option<String> {
    let wrap = iso_meta.child("title-wrap")?;
    let title = wrap.child("full").or_else(|| wrap.child("intro"))?;
    Some(
        inlines::collapse_whitespace(&title.text())
            .trim()
            .to_owned(),
    )
}

fn published(iso_meta: &Element) -> Option<String> {
    let date = iso_meta.child("pub-date")?;
    date.attr("iso-8601-date")
        .map(str::to_owned)
        .or_else(|| Some(date.text().trim().to_owned()))
}

fn namespaces(iso_meta: &Element) -> Vec<String> {
    iso_meta
        .children_named("custom-meta-group")
        .flat_map(meta::pairs)
        .filter(|(name, _)| name == "opc:nodeSet.namespace")
        .map(|(_, value)| value)
        .collect()
}
