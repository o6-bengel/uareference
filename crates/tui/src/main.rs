mod app;
mod corpus;
mod doc;
mod index;
mod platform;
mod search;
mod sync_cmd;
mod ui;

use std::io::stdout;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use clap::{
    Parser,
    Subcommand,
};
use crossterm::cursor::SetCursorStyle;
use crossterm::event::{
    DisableBracketedPaste,
    DisableMouseCapture,
    EnableBracketedPaste,
    EnableMouseCapture,
    Event,
};
use crossterm::{
    event,
    execute,
};
use ratatui::DefaultTerminal;
use uareference_model::Store;

use crate::app::App;

/// Vim-style terminal browser for the OPC UA online reference (reference.opcfoundation.org).
#[derive(Parser)]
#[command(version, subcommand_negates_reqs = true)]
struct Cli {
    /// Data directory (default: $UAREFERENCE_DATA, else the platform data dir).
    #[arg(long, global = true)]
    data: Option<PathBuf>,
    /// Document to open, e.g. `10000-4` or `OPC-10000-100`.
    spec: Option<String>,
    /// Clause to open, e.g. `5.7.2` or `A.1`.
    clause: Option<String>,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Download the specifications and build the local intermediate representation.
    Sync(sync_cmd::SyncArgs),
    /// Print the data directory.
    Path,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let store = match cli.data {
        Some(dir) => Store::at(dir),
        None => Store::open_default()?,
    };
    match cli.command {
        Some(Command::Sync(args)) => sync_cmd::run(&store, &args),
        Some(Command::Path) => {
            println!("{}", store.root().display());
            Ok(())
        }
        None => browse(store, cli.spec.as_deref(), cli.clause.as_deref()),
    }
}

fn browse(
    store: Store,
    spec: Option<&str>,
    clause: Option<&str>,
) -> anyhow::Result<()> {
    let mut app = App::new(store).context("opening the data directory")?;
    let mut terminal = ratatui::init();
    execute!(stdout(), EnableMouseCapture, EnableBracketedPaste, SetCursorStyle::SteadyBlock)?;
    let size = terminal.size()?;
    let areas = ui::areas(ratatui::layout::Rect::new(0, 0, size.width, size.height), &app);
    app.resize(areas.doc.width as usize, areas.doc.height as usize);
    app.start(spec, clause);
    let result = run(&mut terminal, &mut app);
    app.save();
    let _ = execute!(stdout(), DisableMouseCapture, DisableBracketedPaste, SetCursorStyle::DefaultUserShape);
    ratatui::restore();
    result
}

fn run(
    terminal: &mut DefaultTerminal,
    app: &mut App,
) -> anyhow::Result<()> {
    let mut dirty = true;
    let mut bar = false;
    loop {
        if dirty {
            let size = terminal.size()?;
            let areas = ui::areas(ratatui::layout::Rect::new(0, 0, size.width, size.height), app);
            app.resize(areas.doc.width as usize, areas.doc.height as usize);
            if std::mem::take(&mut app.redraw) {
                terminal.clear()?;
            }
            let want_bar = app.cmdline.is_some() || app.picker.is_some();
            if want_bar != bar {
                bar = want_bar;
                let style = if bar {
                    SetCursorStyle::SteadyBar
                } else {
                    SetCursorStyle::SteadyBlock
                };
                execute!(stdout(), style)?;
            }
            terminal.draw(|frame| ui::draw(frame, app))?;
            dirty = false;
        }
        if app.quit {
            return Ok(());
        }
        if event::poll(Duration::from_millis(80))? {
            loop {
                let event = event::read()?;
                if !matches!(event, Event::FocusGained | Event::FocusLost) {
                    dirty = true;
                }
                app.handle(event);
                if app.quit || !event::poll(Duration::ZERO)? {
                    break;
                }
            }
        }
        dirty |= app.tick();
    }
}
