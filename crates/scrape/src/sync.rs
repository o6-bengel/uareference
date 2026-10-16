use std::fs;
use std::sync::{
    Mutex,
    PoisonError,
};

use rayon::prelude::*;
use uareference_model::{
    Catalog,
    CatalogEntry,
    Store,
};

use crate::error::ScrapeError;
use crate::site::{
    Listing,
    Site,
};
use crate::sts::parse_sts;

#[derive(Debug, Clone)]
pub struct SyncOptions {
    /// Site ids to sync; empty means every document.
    pub only: Vec<String>,
    pub force: bool,
    /// Rebuild the IR from the stored STS files without touching the network.
    pub reparse: bool,
    pub jobs: usize,
}

#[derive(Debug, Clone)]
pub enum SyncEvent {
    Catalog { total: usize },
    Skipped { id: String },
    Downloaded { id: String, bytes: usize },
    Parsed { id: String, sections: usize },
    Failed { id: String, error: String },
}

#[derive(Debug, Clone, Default)]
pub struct SyncReport {
    pub updated: Vec<String>,
    pub skipped: Vec<String>,
    pub failed: Vec<(String, String)>,
}

enum Outcome {
    Updated,
    Skipped,
}

type OnEvent<'a> = &'a (dyn Fn(SyncEvent) + Sync);

pub fn sync(
    store: &Store,
    options: &SyncOptions,
    on_event: OnEvent,
) -> Result<SyncReport, ScrapeError> {
    if options.reparse {
        return reparse(store, options, on_event);
    }
    let site = Site::new();
    let mut catalog = store.load_catalog()?;
    merge_listings(&mut catalog, site.listings()?);
    store.save_catalog(&catalog)?;
    let ids = selected(&catalog, &options.only);
    on_event(SyncEvent::Catalog { total: ids.len() });
    let catalog = Mutex::new(catalog);
    let outcomes = run_pool(options.jobs, || {
        ids.par_iter()
            .map(|id| announce(id, fetch_one(&site, store, &catalog, id, options.force, on_event), on_event))
            .collect::<Vec<_>>()
    })?;
    let catalog = catalog.into_inner().unwrap_or_else(PoisonError::into_inner);
    store.save_catalog(&catalog)?;
    Ok(report(outcomes, unknown_ids(&catalog, &options.only), on_event))
}

fn merge_listings(
    catalog: &mut Catalog,
    listings: Vec<Listing>,
) {
    let mut entries = Vec::with_capacity(listings.len());
    for listing in listings {
        let mut entry = catalog
            .get(&listing.id)
            .cloned()
            .unwrap_or_else(|| CatalogEntry {
                id: listing.id.clone(),
                title: String::new(),
                version: String::new(),
                published: None,
                download: None,
                synced: None,
            });
        entry.title = listing.title;
        entry.version = listing.version;
        entries.push(entry);
    }
    catalog.entries = entries;
}

fn selected(
    catalog: &Catalog,
    only: &[String],
) -> Vec<String> {
    catalog
        .entries
        .iter()
        .filter(|e| only.is_empty() || only.contains(&e.id))
        .map(|e| e.id.clone())
        .collect()
}

fn unknown_ids(
    catalog: &Catalog,
    only: &[String],
) -> Vec<String> {
    only.iter()
        .filter(|id| catalog.get(id).is_none())
        .cloned()
        .collect()
}

fn fetch_one(
    site: &Site,
    store: &Store,
    catalog: &Mutex<Catalog>,
    id: &str,
    force: bool,
    on_event: OnEvent,
) -> Result<Outcome, ScrapeError> {
    let landing = site.landing(id)?;
    let synced = update_entry(catalog, id, |entry| {
        entry.title = landing.title.clone().unwrap_or(entry.title.clone());
        entry.version = landing.version.clone().unwrap_or(entry.version.clone());
        entry.published = landing.published.clone();
        entry.download = landing.download.clone();
        entry.synced.clone()
    })
    .flatten();
    let download = landing
        .download
        .ok_or_else(|| ScrapeError::NoDownload { id: id.to_owned() })?;
    if !force && synced.as_deref() == Some(download.as_str()) && store.has_spec(id) {
        return Ok(Outcome::Skipped);
    }
    let bytes = site.bytes(&download)?;
    on_event(SyncEvent::Downloaded {
        id: id.to_owned(),
        bytes: bytes.len(),
    });
    store.save_raw(id, &bytes)?;
    convert(store, id, &bytes, on_event)?;
    update_entry(catalog, id, |entry| entry.synced = Some(download.clone()));
    store.save_catalog(&catalog.lock().unwrap_or_else(PoisonError::into_inner))?;
    Ok(Outcome::Updated)
}

fn update_entry<T>(
    catalog: &Mutex<Catalog>,
    id: &str,
    update: impl FnOnce(&mut CatalogEntry) -> T,
) -> Option<T> {
    let mut catalog = catalog.lock().unwrap_or_else(PoisonError::into_inner);
    catalog.get_mut(id).map(update)
}

fn convert(
    store: &Store,
    id: &str,
    xml: &[u8],
    on_event: OnEvent,
) -> Result<(), ScrapeError> {
    let spec = parse_sts(id, xml)?;
    store.save_spec(&spec)?;
    on_event(SyncEvent::Parsed {
        id: id.to_owned(),
        sections: spec.sections.len(),
    });
    Ok(())
}

fn reparse(
    store: &Store,
    options: &SyncOptions,
    on_event: OnEvent,
) -> Result<SyncReport, ScrapeError> {
    let ids = if options.only.is_empty() {
        raw_ids(store)
    } else {
        options.only.clone()
    };
    on_event(SyncEvent::Catalog { total: ids.len() });
    let outcomes = run_pool(options.jobs, || {
        ids.par_iter()
            .map(|id| {
                let result = store
                    .load_raw(id)
                    .map_err(ScrapeError::from)
                    .and_then(|xml| convert(store, id, &xml, on_event));
                announce(id, result.map(|()| Outcome::Updated), on_event)
            })
            .collect::<Vec<_>>()
    })?;
    Ok(report(outcomes, Vec::new(), on_event))
}

fn raw_ids(store: &Store) -> Vec<String> {
    let Some(dir) = store.raw_path("_").parent().map(ToOwned::to_owned) else {
        return Vec::new();
    };
    let mut ids: Vec<String> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            e.file_name()
                .to_str()?
                .strip_suffix(".xml")
                .map(str::to_owned)
        })
        .collect();
    ids.sort();
    ids
}

fn run_pool<T: Send>(
    jobs: usize,
    work: impl FnOnce() -> T + Send,
) -> Result<T, ScrapeError> {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(jobs.max(1))
        .build()
        .map_err(|e| ScrapeError::Pool(e.to_string()))?;
    Ok(pool.install(work))
}

fn announce(
    id: &str,
    outcome: Result<Outcome, ScrapeError>,
    on_event: OnEvent,
) -> (String, Result<Outcome, String>) {
    let id = id.to_owned();
    let outcome = outcome.map_err(|error| error.to_string());
    match &outcome {
        Ok(Outcome::Skipped) => on_event(SyncEvent::Skipped { id: id.clone() }),
        Err(error) => on_event(SyncEvent::Failed {
            id: id.clone(),
            error: error.clone(),
        }),
        Ok(Outcome::Updated) => {}
    }
    (id, outcome)
}

fn report(
    outcomes: Vec<(String, Result<Outcome, String>)>,
    unknown: Vec<String>,
    on_event: OnEvent,
) -> SyncReport {
    let mut report = SyncReport::default();
    for (id, outcome) in outcomes {
        match outcome {
            Ok(Outcome::Updated) => report.updated.push(id),
            Ok(Outcome::Skipped) => report.skipped.push(id),
            Err(error) => report.failed.push((id, error)),
        }
    }
    for id in unknown {
        let (id, outcome) = announce(&id, Err(ScrapeError::UnknownSpec { id: id.clone() }), on_event);
        if let Err(error) = outcome {
            report.failed.push((id, error));
        }
    }
    report
}
