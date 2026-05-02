mod app;
mod format;
mod sampler;
mod ui;

use std::{
    io::{self, Stdout},
    time::Duration,
};

use anyhow::Result;
use clap::Parser;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::app::App;

#[derive(Debug, Parser)]
#[command(author, version, about = "A lightweight macOS activity monitor TUI")]
struct Args {
    #[arg(
        short,
        long,
        default_value_t = 1000,
        value_name = "MS",
        help = "Refresh interval in milliseconds"
    )]
    interval: u64,

    #[arg(short, long, help = "Start with a process filter")]
    filter: Option<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let interval = Duration::from_millis(args.interval.clamp(250, 10_000));

    let mut terminal = enter_terminal()?;
    let result = App::new(interval, args.filter)?.run(&mut terminal);
    restore_terminal(&mut terminal)?;
    result
}

type CrosstermTerminal = Terminal<CrosstermBackend<Stdout>>;

fn enter_terminal() -> Result<CrosstermTerminal> {
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;
    Ok(terminal)
}

fn restore_terminal(terminal: &mut CrosstermTerminal) -> Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}
