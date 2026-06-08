mod app;
mod error;
mod filter;
mod format;
mod history;
mod inspect;
mod output;
mod ports;
mod sampler;
mod terminal_backend;
mod ui;

use std::{
    env,
    io::{self, BufWriter, Stdout},
    thread,
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
use crate::inspect::InspectOptions;
use crate::output::SnapshotOptions;
use crate::ports::PortOptions;
use crate::sampler::{Sampler, apply_process_trends, collect_process_samples};
use crate::terminal_backend::CrosstermBackend;

const DEFAULT_INTERVAL_MS: u64 = 1_000;
pub const MIN_INTERVAL_MS: u64 = 250;
pub const MAX_INTERVAL_MS: u64 = 10_000;

#[derive(Debug, PartialEq, Eq)]
struct Args {
    interval: u64,
    filter: Option<String>,
    mode: Mode,
}

#[derive(Debug, PartialEq, Eq)]
enum Mode {
    Tui,
    Snapshot(SnapshotMode),
    Ports(PortsMode),
    Inspect(InspectMode),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SnapshotMode {
    json: bool,
    limit: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PortsMode {
    port: Option<u16>,
    json: bool,
    all: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct InspectMode {
    pid: u32,
    json: bool,
    limit: usize,
}

fn main() -> Result<()> {
    let Some(args) = parse_args()? else {
        return Ok(());
    };

    match args.mode {
        Mode::Tui => run_tui(args),
        Mode::Snapshot(mode) => run_snapshot(args.interval, args.filter.as_deref(), mode),
        Mode::Ports(mode) => run_ports(mode),
        Mode::Inspect(mode) => run_inspect(mode),
    }
}

fn run_tui(args: Args) -> Result<()> {
    let interval = Duration::from_millis(args.interval);

    let mut session = TerminalSession::enter()?;
    let result = match App::new(interval, args.filter) {
        Ok(mut app) => app.run(session.terminal_mut()),
        Err(error) => Err(error),
    };
    let restore_result = session.restore();
    restore_result?;
    result
}

fn run_snapshot(interval_ms: u64, filter: Option<&str>, mode: SnapshotMode) -> Result<()> {
    let mut sampler = Sampler::new()?;
    let baseline = sampler.sample(None);
    let previous = collect_process_samples(&baseline.processes);
    thread::sleep(Duration::from_millis(interval_ms));
    let mut snapshot = sampler.sample(None);
    apply_process_trends(&mut snapshot.processes, &previous);
    let rendered = output::render_snapshot(
        &snapshot,
        SnapshotOptions {
            filter,
            limit: mode.limit,
            json: mode.json,
        },
    )?;
    print!("{rendered}");
    if !rendered.ends_with('\n') {
        println!();
    }
    Ok(())
}

fn run_ports(mode: PortsMode) -> Result<()> {
    let options = PortOptions {
        port: mode.port,
        json: mode.json,
        all: mode.all,
    };
    let entries = ports::lookup(options)?;
    let rendered = ports::render(&entries, options)?;
    print!("{rendered}");
    if !rendered.ends_with('\n') {
        println!();
    }
    Ok(())
}

fn run_inspect(mode: InspectMode) -> Result<()> {
    let options = InspectOptions {
        pid: mode.pid,
        json: mode.json,
        limit: mode.limit,
    };
    let inspection = inspect::inspect(options)?;
    let rendered = inspect::render(&inspection, options)?;
    print!("{rendered}");
    if !rendered.ends_with('\n') {
        println!();
    }
    Ok(())
}

fn parse_args() -> Result<Option<Args>> {
    parse_args_from(env::args().skip(1))
}

fn parse_args_from<I, S>(args: I) -> Result<Option<Args>>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let args = args.into_iter().map(Into::into).collect::<Vec<String>>();
    let mut interval = DEFAULT_INTERVAL_MS;
    let mut filter = None;
    let mut snapshot = None;
    let mut index = 0;

    while let Some(arg) = args.get(index) {
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
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(error::message(format!(
                        "{arg} requires a millisecond value"
                    )));
                };
                interval = parse_interval(value)?;
            }
            "-f" | "--filter" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(error::message(format!("{arg} requires a filter value")));
                };
                filter = Some(value.clone());
            }
            _ if arg.starts_with("--interval=") => {
                interval = parse_interval(&arg["--interval=".len()..])?;
            }
            _ if arg.starts_with("--filter=") => {
                filter = Some(arg["--filter=".len()..].to_string());
            }
            "--json" => {
                snapshot
                    .get_or_insert(SnapshotMode {
                        json: false,
                        limit: None,
                    })
                    .json = true;
            }
            "-l" | "--limit" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(error::message(format!("{arg} requires a row count")));
                };
                snapshot
                    .get_or_insert(SnapshotMode {
                        json: false,
                        limit: None,
                    })
                    .limit = Some(parse_limit(value)?);
            }
            _ if arg.starts_with("--limit=") => {
                snapshot
                    .get_or_insert(SnapshotMode {
                        json: false,
                        limit: None,
                    })
                    .limit = Some(parse_limit(&arg["--limit=".len()..])?);
            }
            "snapshot" => {
                return parse_snapshot_args(&args[index + 1..], interval, filter);
            }
            "ports" => {
                return parse_ports_args(&args[index + 1..], interval, filter);
            }
            "inspect" => {
                return parse_inspect_args(&args[index + 1..], interval, filter);
            }
            _ => {
                return Err(error::message(format!(
                    "unknown option: {arg}. Run monitr --help for usage."
                )));
            }
        }
        index += 1;
    }

    Ok(Some(Args {
        interval,
        filter,
        mode: snapshot.map(Mode::Snapshot).unwrap_or(Mode::Tui),
    }))
}

fn parse_snapshot_args(
    args: &[String],
    mut interval: u64,
    mut filter: Option<String>,
) -> Result<Option<Args>> {
    let mut mode = SnapshotMode {
        json: false,
        limit: None,
    };
    let mut index = 0;
    while let Some(arg) = args.get(index) {
        match arg.as_str() {
            "-h" | "--help" => {
                print_snapshot_help();
                return Ok(None);
            }
            "-i" | "--interval" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(error::message(format!(
                        "{arg} requires a millisecond value"
                    )));
                };
                interval = parse_interval(value)?;
            }
            "-f" | "--filter" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(error::message(format!("{arg} requires a filter value")));
                };
                filter = Some(value.clone());
            }
            _ if arg.starts_with("--interval=") => {
                interval = parse_interval(&arg["--interval=".len()..])?;
            }
            _ if arg.starts_with("--filter=") => {
                filter = Some(arg["--filter=".len()..].to_string());
            }
            "--json" => mode.json = true,
            "-l" | "--limit" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(error::message(format!("{arg} requires a row count")));
                };
                mode.limit = Some(parse_limit(value)?);
            }
            _ if arg.starts_with("--limit=") => {
                mode.limit = Some(parse_limit(&arg["--limit=".len()..])?);
            }
            _ => {
                return Err(error::message(format!(
                    "unknown snapshot option: {arg}. Run monitr snapshot --help for usage."
                )));
            }
        }
        index += 1;
    }

    Ok(Some(Args {
        interval,
        filter,
        mode: Mode::Snapshot(mode),
    }))
}

fn parse_ports_args(
    args: &[String],
    interval: u64,
    filter: Option<String>,
) -> Result<Option<Args>> {
    let mut mode = PortsMode {
        port: None,
        json: false,
        all: false,
    };
    for arg in args {
        match arg.as_str() {
            "-h" | "--help" => {
                print_ports_help();
                return Ok(None);
            }
            "--json" => mode.json = true,
            "-a" | "--all" => mode.all = true,
            _ if mode.port.is_none() => mode.port = Some(parse_port(arg)?),
            _ => {
                return Err(error::message(format!(
                    "unknown ports option: {arg}. Run monitr ports --help for usage."
                )));
            }
        }
    }

    Ok(Some(Args {
        interval,
        filter,
        mode: Mode::Ports(mode),
    }))
}

fn parse_inspect_args(
    args: &[String],
    interval: u64,
    filter: Option<String>,
) -> Result<Option<Args>> {
    let mut pid = None;
    let mut json = false;
    let mut limit = 20;
    let mut index = 0;

    while let Some(arg) = args.get(index) {
        match arg.as_str() {
            "-h" | "--help" => {
                print_inspect_help();
                return Ok(None);
            }
            "--json" => json = true,
            "-l" | "--limit" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(error::message(format!("{arg} requires a row count")));
                };
                limit = parse_limit(value)?;
            }
            _ if arg.starts_with("--limit=") => {
                limit = parse_limit(&arg["--limit=".len()..])?;
            }
            _ if pid.is_none() => pid = Some(parse_pid(arg)?),
            _ => {
                return Err(error::message(format!(
                    "unknown inspect option: {arg}. Run monitr inspect --help for usage."
                )));
            }
        }
        index += 1;
    }

    let Some(pid) = pid else {
        return Err(error::message(
            "inspect requires a PID. Run monitr inspect --help for usage.",
        ));
    };

    Ok(Some(Args {
        interval,
        filter,
        mode: Mode::Inspect(InspectMode { pid, json, limit }),
    }))
}

fn parse_interval(value: &str) -> Result<u64> {
    let interval = value.parse().map_err(|_| {
        error::message(format!("invalid interval `{value}`; expected milliseconds"))
    })?;
    if !(MIN_INTERVAL_MS..=MAX_INTERVAL_MS).contains(&interval) {
        return Err(error::message(format!(
            "invalid interval `{value}`; expected {MIN_INTERVAL_MS}-{MAX_INTERVAL_MS} milliseconds"
        )));
    }
    Ok(interval)
}

fn parse_limit(value: &str) -> Result<usize> {
    let limit = value
        .parse()
        .map_err(|_| error::message(format!("invalid limit `{value}`; expected row count")))?;
    if limit == 0 {
        return Err(error::message(
            "invalid limit `0`; expected row count above 0",
        ));
    }
    Ok(limit)
}

fn parse_port(value: &str) -> Result<u16> {
    value
        .parse()
        .map_err(|_| error::message(format!("invalid port `{value}`; expected 1-65535")))
        .and_then(|port| {
            if port == 0 {
                Err(error::message("invalid port `0`; expected 1-65535"))
            } else {
                Ok(port)
            }
        })
}

fn parse_pid(value: &str) -> Result<u32> {
    let pid = value
        .parse()
        .map_err(|_| error::message(format!("invalid pid `{value}`; expected process id")))?;
    if pid == 0 {
        return Err(error::message(
            "invalid pid `0`; expected process id above 0",
        ));
    }
    Ok(pid)
}

fn print_help() {
    println!(
        "\
A lightweight macOS activity monitor TUI

Usage:
  monitr [OPTIONS]
  monitr snapshot [OPTIONS]
  monitr ports [PORT] [OPTIONS]
  monitr inspect <PID> [OPTIONS]

Options:
  -i, --interval <MS>    Refresh interval in milliseconds ({MIN_INTERVAL_MS}-{MAX_INTERVAL_MS}) [default: {DEFAULT_INTERVAL_MS}]
  -f, --filter <FILTER>  Start with a process filter
      --json             Print one machine-readable process snapshot and exit
  -l, --limit <N>        Limit rows for snapshot output
  -h, --help             Print help
  -V, --version          Print version

Commands:
  snapshot                Print a one-shot process snapshot
  ports [PORT]            Show listening TCP sockets, optionally for one port
  inspect <PID>           Show process details, open files, and sockets"
    );
}

fn print_snapshot_help() {
    println!(
        "\
Print a one-shot process snapshot

Usage: monitr snapshot [OPTIONS]

Options:
  -i, --interval <MS>    Sampling window in milliseconds ({MIN_INTERVAL_MS}-{MAX_INTERVAL_MS}) [default: {DEFAULT_INTERVAL_MS}]
  -f, --filter <FILTER>  Filter by PID, name, user, command, or status
      --json             Print JSON
  -l, --limit <N>        Limit process rows
  -h, --help             Print help"
    );
}

fn print_ports_help() {
    println!(
        "\
Show socket ownership by process

Usage: monitr ports [PORT] [OPTIONS]

Options:
      --json             Print JSON
  -a, --all              Include established TCP and UDP sockets, not only TCP listeners
  -h, --help             Print help"
    );
}

fn print_inspect_help() {
    println!(
        "\
Inspect one process

Usage: monitr inspect <PID> [OPTIONS]

Options:
      --json             Print JSON
  -l, --limit <N>        Limit file and socket rows [default: 20]
  -h, --help             Print help"
    );
}

type CrosstermTerminal = Terminal<CrosstermBackend<BufWriter<Stdout>>>;

struct TerminalSession {
    terminal: CrosstermTerminal,
    active: bool,
}

impl TerminalSession {
    fn enter() -> Result<Self> {
        let terminal = enter_terminal()?;
        Ok(Self {
            terminal,
            active: true,
        })
    }

    fn terminal_mut(&mut self) -> &mut CrosstermTerminal {
        &mut self.terminal
    }

    fn restore(&mut self) -> Result<()> {
        restore_terminal(&mut self.terminal)?;
        self.active = false;
        Ok(())
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        if self.active {
            let _ = restore_terminal(&mut self.terminal);
        }
    }
}

fn enter_terminal() -> Result<CrosstermTerminal> {
    enable_raw_mode()?;
    if let Err(error) = execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture) {
        let _ = disable_raw_mode();
        return Err(error.into());
    }
    let backend = CrosstermBackend::new(BufWriter::new(io::stdout()));
    let mut terminal = match Terminal::new(backend) {
        Ok(terminal) => terminal,
        Err(error) => {
            let _ = disable_raw_mode();
            let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
            return Err(error.into());
        }
    };
    if let Err(error) = terminal.clear() {
        let _ = restore_terminal(&mut terminal);
        return Err(error.into());
    }
    Ok(terminal)
}

fn restore_terminal(terminal: &mut CrosstermTerminal) -> Result<()> {
    let raw_result = disable_raw_mode();
    let screen_result = execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    );
    let cursor_result = terminal.show_cursor();

    raw_result?;
    screen_result?;
    cursor_result?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        Args, InspectMode, Mode, PortsMode, SnapshotMode, parse_args_from, parse_interval,
    };

    #[test]
    fn parses_default_args() {
        assert_eq!(
            parse_args_from(Vec::<String>::new()).unwrap(),
            Some(Args {
                interval: 1000,
                filter: None,
                mode: Mode::Tui,
            })
        );
    }

    #[test]
    fn parses_interval_and_filter_args() {
        assert_eq!(
            parse_args_from(["--interval=750", "--filter", "codex"]).unwrap(),
            Some(Args {
                interval: 750,
                filter: Some("codex".to_string()),
                mode: Mode::Tui,
            })
        );
    }

    #[test]
    fn parses_root_json_snapshot_args() {
        assert_eq!(
            parse_args_from(["--filter", "node", "--json", "--limit=5"]).unwrap(),
            Some(Args {
                interval: 1000,
                filter: Some("node".to_string()),
                mode: Mode::Snapshot(SnapshotMode {
                    json: true,
                    limit: Some(5),
                }),
            })
        );
    }

    #[test]
    fn parses_snapshot_subcommand_args() {
        assert_eq!(
            parse_args_from(["snapshot", "--interval", "250", "--json", "--limit", "10"]).unwrap(),
            Some(Args {
                interval: 250,
                filter: None,
                mode: Mode::Snapshot(SnapshotMode {
                    json: true,
                    limit: Some(10),
                }),
            })
        );
    }

    #[test]
    fn parses_ports_subcommand_args() {
        assert_eq!(
            parse_args_from(["ports", "3000", "--all", "--json"]).unwrap(),
            Some(Args {
                interval: 1000,
                filter: None,
                mode: Mode::Ports(PortsMode {
                    port: Some(3000),
                    json: true,
                    all: true,
                }),
            })
        );
    }

    #[test]
    fn parses_inspect_subcommand_args() {
        assert_eq!(
            parse_args_from(["inspect", "1234", "--json", "--limit=7"]).unwrap(),
            Some(Args {
                interval: 1000,
                filter: None,
                mode: Mode::Inspect(InspectMode {
                    pid: 1234,
                    json: true,
                    limit: 7,
                }),
            })
        );
    }

    #[test]
    fn rejects_invalid_interval() {
        let error = parse_interval("fast").unwrap_err().to_string();
        assert!(error.contains("invalid interval"));
    }

    #[test]
    fn rejects_out_of_range_interval() {
        let error = parse_interval("100").unwrap_err().to_string();
        assert!(error.contains("250-10000"));
    }
}
