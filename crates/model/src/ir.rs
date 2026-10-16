use serde::{
    Deserialize,
    Serialize,
};

/// One specification document, flattened into its sections in document order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Spec {
    /// Site identifier, e.g. `OPC-10000-4`.
    pub id: String,
    /// Document number as printed, e.g. `OPC 10000-4`.
    pub document: String,
    pub title: String,
    pub version: String,
    pub published: Option<String>,
    pub namespaces: Vec<String>,
    pub sections: Vec<Section>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Section {
    /// The STS `id` attribute, or a synthesized `auto_<n>` when the source has none.
    pub id: String,
    /// Clause number as printed, e.g. `5.7.2`, `A.1`, `3.1.4`.
    pub label: Option<String>,
    pub title: String,
    pub depth: u8,
    pub parent: Option<u32>,
    pub kind: SectionKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<NodeMeta>,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionKind {
    Foreword,
    Intro,
    Scope,
    NormativeReferences,
    Terms,
    /// A single defined term (`term-sec`); its title is the term.
    Term,
    Symbols,
    Annex,
    Bibliography,
    Clause,
}

/// One `custom-meta-group` of a section's `sec-meta`: a node the section defines or describes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeMeta {
    pub role: NodeRole,
    /// Fully qualified, e.g. `nsu=http://opcfoundation.org/UA/;i=2004`.
    pub node_id: Option<String>,
    pub node_class: Option<String>,
    /// Without the `nsu=…;` prefix, e.g. `ServerType`.
    pub browse_name: Option<String>,
    pub namespace: Option<String>,
    /// Instance declaration path below the type, e.g. `ServerType_ServerArray`.
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conformance_units: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeRole {
    Type,
    Object,
    Component,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Block {
    Paragraph {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default, skip_serializing_if = "is_zero")]
        indent: u8,
        content: Vec<Inline>,
    },
    List {
        ordered: bool,
        items: Vec<Vec<Block>>,
    },
    Table(Table),
    Figure {
        id: Option<String>,
        label: Option<String>,
        caption: Vec<Inline>,
        /// Image path inside the STS package, e.g. `figures/fig_1_….png`.
        image: Option<String>,
    },
    Note {
        label: Option<String>,
        blocks: Vec<Block>,
    },
    Code {
        text: String,
    },
    Reference(Reference),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Table {
    pub id: Option<String>,
    /// e.g. `Table 108`.
    pub label: Option<String>,
    /// Caption text after the label, e.g. `AdditionalParametersType`.
    pub caption: Vec<Inline>,
    pub head: Vec<Row>,
    pub body: Vec<Row>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Row {
    pub cells: Vec<Cell>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cell {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub header: bool,
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub colspan: u16,
    pub blocks: Vec<Block>,
}

/// A bibliography or normative reference entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reference {
    pub id: Option<String>,
    pub label: Option<String>,
    pub citation: Vec<Inline>,
    /// Site identifier when the entry names a document of the online reference.
    pub spec: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum Inline {
    Text(String),
    /// Italic. In OPC UA specifications this marks defined terms and model names, which are
    /// resolved against the symbol index at display time.
    Italic(Vec<Inline>),
    Bold(Vec<Inline>),
    Underline(Vec<Inline>),
    Strike(Vec<Inline>),
    Sup(Vec<Inline>),
    Sub(Vec<Inline>),
    Mono(Vec<Inline>),
    Link {
        target: Link,
        content: Vec<Inline>,
    },
    Image {
        src: String,
    },
    Break,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Link {
    /// An element id inside the same specification: section, table, figure, term or paragraph.
    Anchor {
        id: String,
    },
    /// Another document of the online reference, optionally narrowed to a clause label.
    Spec {
        spec: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        section: Option<String>,
    },
    Url {
        url: String,
    },
}

fn is_zero(v: &u8) -> bool {
    *v == 0
}

fn one() -> u16 {
    1
}

fn is_one(v: &u16) -> bool {
    *v == 1
}
