use uareference_model::store::StoreError;

#[derive(Debug, thiserror::Error)]
pub enum ScrapeError {
    #[error("GET {url}: {message}")]
    Http { url: String, message: String },
    #[error("XML: {0}")]
    Xml(String),
    #[error("the document is not UTF-8")]
    Utf8,
    #[error("{0}")]
    Store(#[from] StoreError),
    #[error("{id}: the landing page offers no STS XML download")]
    NoDownload { id: String },
    #[error("{id}: not in the site catalog")]
    UnknownSpec { id: String },
    #[error("unexpected page layout: {0}")]
    Layout(String),
    #[error("worker pool: {0}")]
    Pool(String),
}
