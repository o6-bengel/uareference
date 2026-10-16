use std::thread;
use std::time::Duration;

use ureq::Agent;

use crate::error::ScrapeError;
use crate::html::{
    between,
    decode_entities,
    href_containing,
    strip_tags,
};

pub const BASE: &str = "https://reference.opcfoundation.org";
const ATTEMPTS: u32 = 3;
const BODY_LIMIT: u64 = 256 * 1024 * 1024;

/// One row of the site's document list.
#[derive(Debug, Clone)]
pub struct Listing {
    pub id: String,
    pub version: String,
    pub title: String,
}

/// What a document's landing page says about its current release.
#[derive(Debug, Clone)]
pub struct Landing {
    pub title: Option<String>,
    pub version: Option<String>,
    pub published: Option<String>,
    pub download: Option<String>,
}

pub struct Site {
    agent: Agent,
}

impl Site {
    pub fn new() -> Self {
        let agent = Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(15)))
            .timeout_global(Some(Duration::from_secs(120)))
            .user_agent(concat!("uareference/", env!("CARGO_PKG_VERSION")))
            .build()
            .into();
        Self { agent }
    }

    pub fn listings(&self) -> Result<Vec<Listing>, ScrapeError> {
        let page = self.text("/")?;
        let listings: Vec<Listing> = page
            .split("class=\"spec-id\"")
            .skip(1)
            .filter_map(listing)
            .collect();
        if listings.is_empty() {
            return Err(ScrapeError::Layout("no documents on the index page".into()));
        }
        Ok(listings)
    }

    pub fn landing(
        &self,
        id: &str,
    ) -> Result<Landing, ScrapeError> {
        let page = self.text(&format!("/specs/{id}"))?;
        let meta = |name: &str| {
            between(&page, &format!("<th scope=\"row\">{name}</th><td>"), "</td>").map(|v| decode_entities(v.trim()))
        };
        Ok(Landing {
            title: between(&page, "<h1>", "</h1>").map(strip_tags),
            version: meta("Release Version"),
            published: meta("Publication Date"),
            download: href_containing(&page, "/download/sts-xml").map(decode_entities),
        })
    }

    pub fn bytes(
        &self,
        path: &str,
    ) -> Result<Vec<u8>, ScrapeError> {
        self.get(path, |response| {
            response
                .body_mut()
                .with_config()
                .limit(BODY_LIMIT)
                .read_to_vec()
        })
    }

    fn text(
        &self,
        path: &str,
    ) -> Result<String, ScrapeError> {
        self.get(path, |response| {
            response
                .body_mut()
                .with_config()
                .limit(BODY_LIMIT)
                .read_to_string()
        })
    }

    fn get<T>(
        &self,
        path: &str,
        read: impl Fn(&mut ureq::http::Response<ureq::Body>) -> Result<T, ureq::Error>,
    ) -> Result<T, ScrapeError> {
        let url = format!("{BASE}{path}");
        let mut attempt = 1;
        loop {
            let result = self
                .agent
                .get(&url)
                .call()
                .and_then(|mut response| read(&mut response));
            match result {
                Ok(value) => return Ok(value),
                Err(error) if attempt < ATTEMPTS && is_transient(&error) => {
                    thread::sleep(Duration::from_millis(500 * u64::from(attempt)));
                    attempt += 1;
                }
                Err(error) => {
                    return Err(ScrapeError::Http {
                        url,
                        message: error.to_string(),
                    });
                }
            }
        }
    }
}

impl Default for Site {
    fn default() -> Self {
        Self::new()
    }
}

fn is_transient(error: &ureq::Error) -> bool {
    match error {
        ureq::Error::StatusCode(code) => *code == 429 || *code >= 500,
        _ => true,
    }
}

fn listing(row: &str) -> Option<Listing> {
    let row = row.split("</li>").next()?;
    let id = between(row, "href=\"/specs/", "\"")?;
    Some(Listing {
        id: decode_entities(id),
        version: between(row, "class=\"version-tag\">", "<")
            .map(decode_entities)
            .unwrap_or_default(),
        title: between(row, "class=\"spec-title\">", "<")
            .map(decode_entities)
            .unwrap_or_default(),
    })
}
