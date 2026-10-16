use uareference_model::{
    NodeMeta,
    NodeRole,
};

use crate::dom::Element;

pub fn node_metas(sec_meta: &Element) -> Vec<NodeMeta> {
    sec_meta
        .children_named("custom-meta-group")
        .map(node_meta)
        .collect()
}

fn node_meta(group: &Element) -> NodeMeta {
    let mut meta = NodeMeta {
        role: NodeRole::Other,
        node_id: None,
        node_class: None,
        browse_name: None,
        namespace: None,
        path: None,
        conformance_units: Vec::new(),
    };
    for (name, value) in pairs(group) {
        match name.as_str() {
            "opc:role" => meta.role = role(&value),
            "opc:nodeId" => meta.node_id = Some(value),
            "opc:nodeClass" => meta.node_class = Some(value),
            "opc:pathName" => meta.path = Some(value),
            "opc:conformanceUnit" => meta.conformance_units.push(value),
            "opc:browseName" => {
                let (namespace, name) = split_browse_name(&value);
                meta.namespace = namespace;
                meta.browse_name = Some(name);
            }
            _ => {}
        }
    }
    meta
}

/// `(name, value)` of every `custom-meta` in a group.
pub fn pairs(group: &Element) -> impl Iterator<Item = (String, String)> + '_ {
    group.children_named("custom-meta").map(|meta| {
        let text = |name| {
            meta.child(name)
                .map(|e| e.text().trim().to_owned())
                .unwrap_or_default()
        };
        (text("meta-name"), text("meta-value"))
    })
}

fn role(value: &str) -> NodeRole {
    match value {
        "Type" => NodeRole::Type,
        "Object" => NodeRole::Object,
        "Component" => NodeRole::Component,
        _ => NodeRole::Other,
    }
}

fn split_browse_name(value: &str) -> (Option<String>, String) {
    match value
        .strip_prefix("nsu=")
        .and_then(|rest| rest.split_once(';'))
    {
        Some((namespace, name)) => (Some(namespace.to_owned()), name.to_owned()),
        None => (None, value.to_owned()),
    }
}
