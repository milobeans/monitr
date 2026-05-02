mod app;
mod error;
mod format;
mod sampler;
mod terminal_backend;
mod ui;

use std::{
    env,
    io::{self, Stdout},
    time::Duration,
};

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui_core::terminal::Terminal;

use crate::app::App;
use crate::error::Result;
use crate::terminal_backend::CrosstermBackend;

struct Args {
    interval: u64,
    filter: Option<String>,
}

fn main() -> Result<()> {
    let Some(args) = parse_args()? else {
        return Ok(());
    };
    let interval = Duration::from_millis(args.interval.clamp(250, 10_000));

    let mut terminal = enter_terminal()?;
    let result = App::new(interval, args.filter)?.run(&mut terminal);
    restore_terminal(&mut terminal)?;
    result
}

fn parse_args() -> Result<Option<Args>> {
    let mut args = env::args().skip(1);
    let mut interval = 1000;
    let mut filter = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print_help();
                return Ok(None);
            }
            "-V" | "--version" => {
                println!("monitr {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            "-i" | "--interval" => {
                let Some(value) = args.next() else {
                    return Err(error::message(format!(
                        "{arg} requires a millisecond value"
                    )));
                };
                interval = parse_interval(&value)?;
            }
            "-f" | "--filter" => {
                let Some(value) = args.next() else {
                    return Err(error::message(format!("{arg} requires a filter value")));
                };
                filter = Some(value);
            }
            _ if arg.starts_with("--interval=") => {
                interval = parse_interval(&arg["--interval=".len()..])?;
            }
            _ if arg.starts_with("--filter=") => {
                filter = Some(arg["--filter=".len()..].to_string());
            }
            _ => {
                return Err(error::message(format!(
                    "unknown option: {arg}. Run monitr --help for usage."
                )));
            }
        }
    }

    Ok(Some(Args { interval, filter }))
}

fn parse_interval(value: &str) -> Result<u64> {
    value
        .parse()
        .map_err(|_| error::message(format!("invalid interval `{value}`; expected milliseconds")))
}

fn print_help() {
    println!(
        "\
A lightweight macOS activity monitor TUI

Usage: monitr [OPTIONS]

Options:
  -i, --interval <MS>    Refresh interval in milliseconds [default: 1000]
  -f, --filter <FILTER>  Start with a process filter
  -h, --help             Print help
  -V, --version          Print version"
    );
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
