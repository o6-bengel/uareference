use std::sync::atomic::{
    AtomicUsize,
    Ordering,
};

use anyhow::bail;
use uareference_model::Store;
use uareference_model::text::normalize_spec_id;
use uareference_scrape::{
    SyncEvent,
    SyncOptions,
};

#[derive(clap::Args)]
pub struct SyncArgs {
    /// Documents to sync, e.g. `10000-4 10000-5`; all documents when omitted.
    specs: Vec<String>,
    /// Download and rebuild even when the site reports no change.
    #[arg(short, long)]
    force: bool,
    /// Rebuild the IR from the downloaded XML without touching the network.
    #[arg(long)]
    reparse: bool,
    /// Parallel downloads.
    #[arg(short, long, default_value_t = 4)]
    jobs: usize,
}

pub fn run(
    store: &Store,
    args: &SyncArgs,
) -> anyhow::Result<()> {
    let mut only = Vec::new();
    for spec in &args.specs {
        match normalize_spec_id(spec) {
            Some(id) => only.push(id),
            None => bail!("not a document number: {spec}"),
        }
    }
    let options = SyncOptions {
        only,
        force: args.force,
        reparse: args.reparse,
        jobs: args.jobs.max(1),
    };
    let total = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let progress = |id: &str, what: &str| {
        let n = done.fetch_add(1, Ordering::Relaxed) + 1;
        let total = total.load(Ordering::Relaxed);
        println!("[{n:>3}/{total}] {id:<16} {what}");
    };
    let on_event = |event: SyncEvent| match event {
        SyncEvent::Catalog { total: n } => {
            total.store(n, Ordering::Relaxed);
            println!("{n} documents to check in {}", store.root().display());
        }
        SyncEvent::Skipped { id } => progress(&id, "up to date"),
        SyncEvent::Downloaded { .. } => {}
        SyncEvent::Parsed { id, sections } => progress(&id, &format!("{sections} sections")),
        SyncEvent::Failed { id, error } => progress(&id, &format!("FAILED: {error}")),
    };
    let report = uareference_scrape::sync(store, &options, &on_event)?;
    println!(
        "{} updated, {} unchanged, {} failed",
        report.updated.len(),
        report.skipped.len(),
        report.failed.len()
    );
    if !report.failed.is_empty() {
        bail!("{} document(s) failed", report.failed.len());
    }
    Ok(())
}
