# uareference

A fast, offline, vim-style terminal browser for the [OPC UA Online Reference](https://reference.opcfoundation.org).
It downloads every published specification once, turns it into a structured intermediate representation (IR), and lets
you read and navigate it like code: `gd` to a definition, `gr` for references, `K` to peek, `<C-o>` to jump back, and
fuzzy pickers for documents, symbols, NodeIds, terms, headings and full-text search across all documents.

## Install and first run

```sh
cargo install --path crates/tui        # installs the `uareference` binary
uareference sync                       # ~20 s: 148 documents, ~180 MB in ~/.local/share/uareference
uareference                            # resumes where you left off
uareference 10000-4 5.7.2              # opens OPC 10000-4, clause 5.7.2
```

`uareference sync` is incremental: a document is only downloaded again when the site publishes a new revision.
`sync 10000-4 10000-5` limits it to some documents, `--force` rebuilds anyway, `--reparse` rebuilds the IR from the
already downloaded XML without touching the network. Inside the browser, `:sync` does the same in the background.
The data directory is `$UAREFERENCE_DATA`, `--data <dir>`, or the platform default (`uareference path` prints it).

## Navigation

The cursor moves like in Vim, with counts: `h j k l`, `w b e W B E`, `0 ^ $`, `gg G` (or `{n}G`), `{n}%`, `H M L`,
`{ }`, `f F T ; ,`, `<C-d> <C-u> <C-f> <C-b> <C-e> <C-y>`, `zt zz zb`. `t` starts the tab commands, not a motion.

| Keys | Action |
| --- | --- |
| `gd`, `<C-]>`, `<CR>` | Go to the definition of the term, link or word under the cursor |
| `gD` | List every definition candidate |
| `gr` | References to the symbol under the cursor, across all documents (on a heading: to that section) |
| `K` | Peek at the definition in a popup (`K`/`<CR>` again jumps there) |
| `gx` | Open the link, figure or current section in the browser |
| `<C-o>` `<C-t>` `<BS>` / `<Tab>` | Jump back / forward |
| `<C-^>` | Alternate document |
| `tt` / `td` | New tab at the current position / close the tab |
| `tk` `tn` / `tj`, `th` / `tl` | Next / previous tab (with a count: that many), first / last tab |
| `]]` `[[` `[u` | Next / previous / parent section |
| `]t` `[t`, `]f` `[f`, `]l` `[l` | Next / previous table, figure, link or term |
| `gO`, `<Space>o` | Outline of the current document |
| `<Space>f`, `<Space><Space>` | Find a document |
| `<Space>s` | Workspace symbols: types, objects, instance declarations, tables; type `i=2004` for a NodeId |
| `<Space>t` / `<Space>h` | Defined terms / headings of all documents |
| `<Space>/`, `<Space>g` | Full-text search across all documents (live) |
| `<Space>w` | Full-text search for the word under the cursor |
| `/` `?` `n` `N` `*` `#` | Search in the document (smart case, incremental; `\<word\>` for whole words) |
| `m{a-z}` / `m{A-Z}` | Set a mark in this document / a global mark |
| `'{mark}` `` `{mark} `` `''` | Jump to a mark (line / exact) / back to before the last jump |
| `<Space>m` / `<Space>j` | Marks / jump list pickers |
| `v` `V` then `y` | Visual selection and yank |
| `yy` `yu` `yc` `yn` | Yank line, section URL, citation (`OPC 10000-4, 5.7.2 CreateSession`), NodeId |
| `gh` | Home: the list of all documents |
| `<Space>e` | Toggle the outline sidebar |
| `<Space>?` | Searchable list of all key bindings (`<CR>` runs one) |
| `<C-g>` | Document info and URL in a popup (`<CR>` opens the URL) |

Yanking uses OSC 52, so it reaches the system clipboard from WSL, SSH and tmux (`set -g set-clipboard on`).
After any prefix key (`g`, `<Space>`, `]`, `z`, `y`, …) a hint lists the possible continuations.

In pickers: type to filter (fuzzy, smart case), `<C-n>`/`<C-p>` or arrows to move, `<CR>` to open, `<C-d>`/`<C-u>`
to scroll the preview, `<Esc>` to close.

## Commands

| Command | Action |
| --- | --- |
| `:e 10000-4 [5.7.2]` | Open a document (number, id or part of its title), optionally at a clause |
| `:5.7.2`, `:A.1` | Go to a clause of the current document |
| `:def ServerType` | Go to a definition by name |
| `:node i=2004` | Go to the node with this NodeId |
| `:grep monitored item` | Full-text search |
| `:refs`, `:outline`, `:specs`, `:symbols`, `:terms`, `:headings`, `:marks`, `:jumps` | Pickers |
| `:delmarks a b` / `:delmarks!` | Delete marks |
| `:set tw=100`, `:set so=999`, `:set outline` / `nooutline` | Prose width, scroll offset (999, the default, keeps the cursor centered), sidebar |
| `:noh` | Clear the search highlight |
| `:web` | Open the current section in the browser |
| `:sync[!] [10000-4 …]` | Sync in the background (`!` forces a rebuild) |
| `:reload`, `:home`, `:help`, `:q` | |

Each tab has its own position, jump list and alternate document. Command and search history (`<Up>`/`<Down>`),
marks, the last position in every document and settings persist in `state.json` in the data directory; tabs do not.

## How definitions are found

OPC UA specifications set defined terms and model names in italics. Every italic phrase is a potential symbol, as is
every word under the cursor. They are resolved at display time against an index built from all documents:

1. node definitions from the specification metadata (ObjectTypes, VariableTypes, DataTypes, ReferenceTypes),
2. defined terms (clause 3 of each document),
3. section titles (`CreateSession`, `Browse`, …),
4. objects, methods and tables named after a single identifier.

Plurals and possessives are folded (`Servers`, `Server's` → `Server`). The kinds above rank in that order; within a
kind, the current document wins over the core specification (OPC 10000-1 … 26), which wins over companion
specifications. `gD` shows all candidates.

## Intermediate representation

`specs/<id>.json` in the data directory is one document, self-contained and independent of every other document, so
other tools can use it (`jq '.sections[] | select(.label == "5.7.2")' specs/OPC-10000-4.json`).

- `Spec` — `id`, `document`, `title`, `version`, `published`, `namespaces`, and `sections` flattened in document order.
- `Section` — `id`, `label` (`5.7.2`, `A.1`), `title`, `depth`, `parent`, `kind` (clause, term, annex, …), `nodes`
  (NodeId, BrowseName, NodeClass, instance paths, conformance units from the specification metadata) and `blocks`.
- `Block` — paragraph, list, table (head/body rows, header cells, colspans), figure, note, code, reference.
- `Inline` — text, italic, bold, underline, strike, sup, sub, mono, image, break, and `link`, whose target is an
  `anchor` in the same document, another `spec` (optionally with a clause `section`), or a `url`.

Cross-document links stay symbolic (`OPC-10000-3`, clause `5.2`), so re-syncing one document never leaves links in
another one stale.

## Architecture

| Crate | Purpose |
| --- | --- |
| `crates/model` (`uareference-model`) | IR types, the catalog, the on-disk store, plain-text helpers |
| `crates/scrape` (`uareference-scrape`) | Site catalog and download (ureq), NISO STS XML → IR transform (quick-xml), sync |
| `crates/tui` (`uareference`) | The browser (ratatui): rendering, index, search, keymap, pickers, and the `sync` command |

The scraper reads the NISO STS XML each document offers for download rather than the HTML, because it carries the
document structure, stable element ids for cross-references, and the node metadata behind `gd`.

Figures are not drawn in the terminal; `gx` on a figure opens it on the website.

The specifications are © OPC Foundation and subject to its [specification license](https://opcfoundation.org/license/specifications/);
this tool only keeps a local copy for your own reading.
