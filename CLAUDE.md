# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

`uareference` is a vim-style terminal browser for the OPC UA online reference (reference.opcfoundation.org). See
README.md for the user-facing behaviour.

## Code guidelines

1. Prefer small, descriptive functions.
2. Add as few comments as possible. At most a single sentence per struct or function, and only if it is ABSOLUTELY
   necessary.
3. Do not add any unit or integration tests unless specifically asked to.
4. Delegate liberally to subagents: fan out searches, research, and independent pieces of work.
5. Never guess at OPC UA semantics; use the `opc-ua-reference` MCP server or the synced data.

## Commands

- Build: `cargo build --release` (the TUI is slow in debug builds on large documents)
- Lint: `cargo clippy --workspace --all-targets`
- Format: `cargo +nightly fmt --all` — `rustfmt.toml` uses nightly-only options
- Sync data into a scratch directory: `uareference --data <dir> sync [ids…]`; `--reparse` rebuilds the IR offline
  after a parser change.

## Architecture

- `crates/model` — the IR (`ir.rs`), the catalog, the `Store` (data directory layout: `catalog.json`,
  `raw/<id>.xml`, `specs/<id>.json`, `state.json`). Pure data; no network.
- `crates/scrape` — `site.rs` fetches the index page, landing pages and STS XML; `sts/` turns STS into the IR;
  `sync.rs` orchestrates (incremental via the download token in the STS URL).
- `crates/tui` — `corpus.rs` (loaded documents, anchors), `index.rs` (symbols and references), `search.rs`
  (full-text index and the background grep worker), `doc/` (IR → wrapped, styled lines), `app/` (state, keymap,
  actions, pickers, command line, tabs), `ui/` (drawing).

Invariants worth knowing before changing things:

- The IR of one document never depends on another document. Italic terms are stored as text and resolved at
  display time against `Index`; cross-document links are `Link::Spec { spec, section }`. Keep it that way so
  syncing one document cannot leave another stale.
- `corpus::for_each_ref` defines the canonical order of the linkable inlines in a block. The reference index
  records occurrences as (section, slot, nth) in that order, and the renderer tags spans with the same ordinal
  (`ref_ordinals`, keyed by inline address), which is how `gr` results land on the exact span.
- Positions that must survive re-wrapping (marks, jumps, last position) are `Place`s holding a `doc::Origin`
  (section, slot, row), never screen lines. Slot 0 is a section's heading, slot n is block n − 1.
- The active tab lives in `App::view`, `alternate`, `jumps` and `last_jump`; `App::tabs` holds only the background
  tabs, keyed by `Place` so they survive re-rendering and library reloads.
- Startup paints the requested document first; a background thread loads every document, builds the index and
  the search text, and `App::install` swaps them in. Anything needing the index says "still indexing" until then.
