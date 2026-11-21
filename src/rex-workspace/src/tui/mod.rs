//! Full-screen workspace manager.

mod app;
pub mod procs;
mod ui;

pub use app::{Action, Launch};

use crate::manifest::Env;
use crate::rex::Rex;
use anyhow::{bail, Result};
use crossterm::event::{self, Event, KeyEventKind};
use std::time::{Duration, Instant};

/// The hub stays open for hours; keep it current without a keypress.
const REFRESH: Duration = Duration::from_secs(5);

/// Run the TUI; returns lines to print after the terminal is restored.
pub fn run(env: Env, rex: Rex, launch: Launch) -> Result<Vec<String>> {
    if let claw::TerminalStatus::Unavailable(reason) = claw::TerminalStatus::detect() {
        bail!("the interactive view needs a terminal ({reason}); name the set and repos explicitly");
    }
    let mut app = app::App::new(env, rex);
    app.launch(launch);
    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, &mut app);
    ratatui::restore();
    result?;
    Ok(app.notes)
}

fn event_loop(terminal: &mut ratatui::DefaultTerminal, app: &mut app::App) -> Result<()> {
    let mut refreshed = Instant::now();
    while !app.quit {
        if app.hub && app.modal.is_none() && refreshed.elapsed() >= REFRESH {
            app.reload(None);
            refreshed = Instant::now();
        }
        app.drain_details();
        terminal.draw(|f| ui::render(f, app))?;
        if let Some(job) = app.pending.take() {
            app.status = Some(app::Status { text: job.describe(), error: false });
            terminal.draw(|f| ui::render(f, app))?;
            app.execute(job);
            continue;
        }
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Release {
                    app.key(key);
                }
            }
        }
    }
    Ok(())
}
