//! jarvis-term: full-screen dashboard for the Jarvis server's TTY.
//! Two colors (green + white), ASCII only, single thread, polls files and /proc.

mod app;
mod parse;
mod services;
mod sys;
mod tail;
mod telegram;
mod ui;
mod util;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

use app::App;

const HELP: &str = "jarvis-term - Jarvis server dashboard

Usage: jarvis-term

Keys:  q / Esc        quit
       Tab, 1 2 3     focus API CALLS / TELEGRAM / LOGS
       Up Down        scroll one line (k j)
       PgUp PgDn      scroll a page
       Home / End     oldest / follow newest

Env:   HERMES_HOME    Hermes directory (default: $HOME/.hermes)";

fn hermes_home() -> PathBuf {
    if let Ok(h) = std::env::var("HERMES_HOME") {
        return PathBuf::from(h);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
    PathBuf::from(home).join(".hermes")
}

fn main() -> std::io::Result<()> {
    if let Some(arg) = std::env::args().nth(1) {
        match arg.as_str() {
            "-V" | "--version" => println!("jarvis-term {}", env!("CARGO_PKG_VERSION")),
            _ => println!("{HELP}"),
        }
        return Ok(());
    }

    let mut app = App::new(&hermes_home());
    // Restores the terminal on panic too (installs its own hook).
    let mut terminal = ratatui::init();
    let res = run(&mut terminal, &mut app);
    ratatui::restore();
    res
}

fn run(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> std::io::Result<()> {
    let long_ago = Instant::now() - Duration::from_secs(60);
    let (mut t_fast, mut t_slow, mut t_logs, mut t_db) = (long_ago, long_ago, long_ago, long_ago);
    let mut dirty = true;

    while !app.quit {
        let now = Instant::now();
        if now - t_slow >= Duration::from_secs(10) {
            app.tick_slow();
            t_slow = now;
            dirty = true;
        }
        if now - t_fast >= Duration::from_secs(1) {
            app.tick_fast();
            t_fast = now;
            dirty = true;
        }
        if now - t_logs >= Duration::from_millis(500) {
            dirty |= app.poll_logs();
            t_logs = now;
        }
        if now - t_db >= Duration::from_secs(2) {
            dirty |= app.poll_telegram();
            t_db = now;
        }

        if dirty {
            terminal.draw(|f| ui::draw(f, app))?;
            dirty = false;
        }

        if event::poll(Duration::from_millis(250))? {
            match event::read()? {
                Event::Key(k) if k.kind == KeyEventKind::Press => {
                    if k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL) {
                        app.quit = true;
                    } else {
                        app.key(k.code);
                    }
                    dirty = true;
                }
                Event::Resize(..) => dirty = true,
                _ => {}
            }
        }
    }
    Ok(())
}
