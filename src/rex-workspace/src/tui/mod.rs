//! Full-screen workspace manager.

mod app;
mod picker;
mod query;
mod ui;

pub use app::{Action, Launch};

use crate::manifest::Env;
use crate::rex::Rex;
use anyhow::{bail, Result};
use crossterm::event::{self, Event, KeyEventKind};
use std::io::IsTerminal;
use std::time::Duration;

/// Run the TUI; returns lines to print after the terminal is restored.
pub fn run(env: Env, rex: Rex, launch: Launch) -> Result<Vec<String>> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        bail!("the interactive view needs a terminal; name the set and repos explicitly");
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
    while !app.quit {
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
