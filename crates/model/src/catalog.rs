use serde::{
    Deserialize,
    Serialize,
};

/// The list of documents the site publishes, and what the local store holds of each.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Catalog {
    pub entries: Vec<CatalogEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogEntry {
    /// Site identifier, e.g. `OPC-10000-4`.
    pub id: String,
    pub title: String,
    pub version: String,
    pub published: Option<String>,
    /// Site path of the STS XML download; it embeds a publication token, so a change means new content.
    pub download: Option<String>,
    /// The `download` value the stored IR was built from.
    pub synced: Option<String>,
}

impl Catalog {
    pub fn get(
        &self,
        id: &str,
    ) -> Option<&CatalogEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    pub fn get_mut(
        &mut self,
        id: &str,
    ) -> Option<&mut CatalogEntry> {
        self.entries.iter_mut().find(|e| e.id == id)
    }
}

impl CatalogEntry {
    pub fn is_stale(&self) -> bool {
        self.download.is_some() && self.download != self.synced
    }
}
