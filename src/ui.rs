use ratatui_core::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    terminal::Frame,
    text::{Line, Span},
};
use ratatui_widgets::{
    block::Block,
    borders::Borders,
    clear::Clear,
    gauge::Gauge,
    paragraph::{Paragraph, Wrap},
    table::{Cell, Row, Table, TableState},
    tabs::Tabs,
};

use crate::{
    app::{App, Tab},
    format,
    sampler::{DiskRow, NetworkRow, ProcessRow, Snapshot},
};

const BG: Color = Color::Rgb(12, 14, 16);
const PANEL: Color = Color::Rgb(24, 27, 31);
const PANEL_ALT: Color = Color::Rgb(34, 38, 43);
const TEXT: Color = Color::Rgb(220, 224, 229);
const MUTED: Color = Color::Rgb(132, 142, 152);
const GREEN: Color = Color::Rgb(69, 190, 132);
const BLUE: Color = Color::Rgb(83, 153, 220);
const YELLOW: Color = Color::Rgb(224, 176, 72);
const RED: Color = Color::Rgb(224, 87, 87);

pub fn draw(frame: &mut Frame<'_>, app: &mut App) {
    let area = frame.area();
    frame.render_widget(Block::default().style(Style::default().bg(BG)), area);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(5),
            Constraint::Min(8),
            Constraint::Length(1),
        ])
        .split(area);

    render_title(frame, app, layout[0]);
    render_tabs(frame, app, layout[1]);
    render_overview(frame, app.snapshot(), layout[2]);
    render_main(frame, app, layout[3]);
    render_footer(frame, app, layout[4]);

    if app.show_help {
        render_help(frame, area);
    }
}

fn render_title(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let snapshot = app.snapshot();
    let (status, status_style) = app
        .confirm
        .map(|intent| {
            let pid = app
                .selected_pid()
                .map(|pid| pid.to_string())
                .unwrap_or_else(|| "-".to_string());
            (
                format!("Confirm {} pid {}: y/n", intent.label(), pid),
                Style::default().fg(RED).add_modifier(Modifier::BOLD),
            )
        })
        .or_else(|| {
            app.notice
                .as_ref()
                .map(|notice| (notice.text().to_string(), Style::default().fg(YELLOW)))
        })
        .unwrap_or_else(|| {
            (
                format!(
                    "{} of {} shown | sort {} {} | refresh {} ms | sample {:.2}s | uptime {}",
                    app.visible_count(),
                    app.process_count(),
                    app.sort_key.title(),
                    if app.sort_desc { "desc" } else { "asc" },
                    app.interval().as_millis(),
                    snapshot.sample_span.as_secs_f64(),
                    format::duration(snapshot.totals.uptime)
                ),
                Style::default().fg(MUTED),
            )
        });
    let filter = if app.filter_mode {
        format!("filter: {}_", app.filter)
    } else if app.filter.is_empty() {
        "filter: none".to_string()
    } else {
        format!("filter: {}", app.filter)
    };
    let line = Line::from(vec![
        Span::styled(
            " monitr ",
            Style::default()
                .fg(BG)
                .bg(GREEN)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(&snapshot.totals.host, Style::default().fg(TEXT)),
        Span::styled("  ", Style::default().fg(MUTED)),
        Span::styled(&snapshot.totals.os, Style::default().fg(MUTED)),
        Span::styled("  ", Style::default().fg(MUTED)),
        Span::styled(filter, Style::default().fg(BLUE)),
        Span::styled("  ", Style::default().fg(MUTED)),
        Span::styled(status, status_style),
    ]);
    frame.render_widget(Paragraph::new(line).style(Style::default().bg(BG)), area);
}

fn render_tabs(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let titles = Tab::ALL
        .iter()
        .map(|tab| Line::from(tab.title()))
        .collect::<Vec<_>>();
    let selected = Tab::ALL.iter().position(|tab| *tab == app.tab).unwrap_or(0);
    let tabs = Tabs::new(titles)
        .select(selected)
        .block(Block::default().borders(Borders::ALL).style(panel_style()))
        .style(Style::default().fg(MUTED))
        .highlight_style(
            Style::default()
                .fg(GREEN)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        );
    frame.render_widget(tabs, area);
}

fn render_overview(frame: &mut Frame<'_>, snapshot: &Snapshot, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ])
        .split(area);

    let cpu_ratio = (snapshot.totals.cpu_usage as f64 / 100.0).clamp(0.0, 1.0);
    let cpu = Gauge::default()
        .block(metric_block("CPU"))
        .gauge_style(Style::default().fg(usage_color(snapshot.totals.cpu_usage as f64)))
        .ratio(cpu_ratio)
        .label(format!(
            "{} across {} cores",
            format::percent(snapshot.totals.cpu_usage as f64),
            snapshot.totals.cpu_count
        ));
    frame.render_widget(cpu, chunks[0]);

    let memory_ratio = if snapshot.totals.total_memory > 0 {
        snapshot.totals.used_memory as f64 / snapshot.totals.total_memory as f64
    } else {
        0.0
    };
    let memory = Gauge::default()
        .block(metric_block("Memory"))
        .gauge_style(Style::default().fg(usage_color(memory_ratio * 100.0)))
        .ratio(memory_ratio.clamp(0.0, 1.0))
        .label(format!(
            "{} / {}  swap {} / {}",
            format::bytes(snapshot.totals.used_memory),
            format::bytes(snapshot.totals.total_memory),
            format::bytes(snapshot.totals.used_swap),
            format::bytes(snapshot.totals.total_swap)
        ));
    frame.render_widget(memory, chunks[1]);

    let disk = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("Read  ", Style::default().fg(MUTED)),
            Span::styled(
                format::bytes_rate(snapshot.totals.disk_read_rate),
                value_style(BLUE),
            ),
        ]),
        Line::from(vec![
            Span::styled("Write ", Style::default().fg(MUTED)),
            Span::styled(
                format::bytes_rate(snapshot.totals.disk_write_rate),
                value_style(YELLOW),
            ),
        ]),
    ])
    .block(metric_block("Disk"))
    .style(Style::default().fg(TEXT));
    frame.render_widget(disk, chunks[2]);

    let network = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("In  ", Style::default().fg(MUTED)),
            Span::styled(
                format::bytes_rate(snapshot.totals.net_in_rate),
                value_style(GREEN),
            ),
        ]),
        Line::from(vec![
            Span::styled("Out ", Style::default().fg(MUTED)),
            Span::styled(
                format::bytes_rate(snapshot.totals.net_out_rate),
                value_style(BLUE),
            ),
        ]),
    ])
    .block(metric_block("Network"))
    .style(Style::default().fg(TEXT));
    frame.render_widget(network, chunks[3]);
}

fn render_main(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    if app.show_details && area.width >= 104 {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(64), Constraint::Length(42)])
            .split(area);
        render_process_table(frame, app, chunks[0]);
        render_details(frame, app, chunks[1]);
    } else if app.show_details && area.height >= 20 {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(8), Constraint::Length(10)])
            .split(area);
        render_process_table(frame, app, chunks[0]);
        render_details(frame, app, chunks[1]);
    } else {
        render_process_table(frame, app, area);
    }
}

fn render_process_table(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    let (headers, widths) = table_schema(app.tab, app.sort_key, app.sort_desc);
    let header = Row::new(headers.iter().cloned().map(Cell::from).collect::<Vec<_>>())
        .style(
            Style::default()
                .fg(TEXT)
                .bg(PANEL_ALT)
                .add_modifier(Modifier::BOLD),
        )
        .height(1);

    let rows = app
        .visible
        .iter()
        .filter_map(|index| app.snapshot().processes.get(*index))
        .map(|process| process_row(process, app.tab))
        .collect::<Vec<_>>();

    let title = process_table_title(app);
    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .style(panel_style()),
        )
        .row_highlight_style(
            Style::default()
                .fg(Color::White)
                .bg(Color::Rgb(51, 74, 94))
                .add_modifier(Modifier::BOLD),
        )
        .column_spacing(1);
    render_stateful_table(frame, table, area, &mut app.table_state);
}

fn process_table_title(app: &App) -> String {
    let base = match app.tab {
        Tab::Disk => "Disk activity by process".to_string(),
        Tab::Network => "Process context | interface totals in inspector".to_string(),
        _ => app.tab.title().to_string(),
    };
    if app.visible_count() == 0 && !app.filter.is_empty() {
        return format!(
            "{} | no matches for {}",
            base,
            format::truncate_middle(&app.filter, 24)
        );
    }
    let selection = app
        .selected_position()
        .map(|selected| format!(" | selected {selected}/{}", app.visible_count()))
        .unwrap_or_default();
    format!("{} | {} visible{}", base, app.visible_count(), selection)
}

fn render_stateful_table(
    frame: &mut Frame<'_>,
    table: Table<'_>,
    area: Rect,
    state: &mut TableState,
) {
    frame.render_stateful_widget(table, area, state);
}

fn table_schema(
    tab: Tab,
    sort_key: crate::app::SortKey,
    sort_desc: bool,
) -> (Vec<String>, Vec<Constraint>) {
    let (headers, widths) = match tab {
        Tab::Cpu => (
            vec![
                sort_header("PID", sort_key == crate::app::SortKey::Pid, sort_desc),
                sort_header("Process", sort_key == crate::app::SortKey::Name, sort_desc),
                sort_header("User", sort_key == crate::app::SortKey::User, sort_desc),
                sort_header("% CPU", sort_key == crate::app::SortKey::Cpu, sort_desc),
                sort_header("Memory", sort_key == crate::app::SortKey::Memory, sort_desc),
                sort_header("Time", sort_key == crate::app::SortKey::Runtime, sort_desc),
                "Status".to_string(),
            ],
            vec![
                Constraint::Length(7),
                Constraint::Min(22),
                Constraint::Length(13),
                Constraint::Length(8),
                Constraint::Length(10),
                Constraint::Length(10),
                Constraint::Length(10),
            ],
        ),
        Tab::Memory => (
            vec![
                sort_header("PID", sort_key == crate::app::SortKey::Pid, sort_desc),
                sort_header("Process", sort_key == crate::app::SortKey::Name, sort_desc),
                sort_header("User", sort_key == crate::app::SortKey::User, sort_desc),
                sort_header("Memory", sort_key == crate::app::SortKey::Memory, sort_desc),
                "% Mem".to_string(),
                "Virtual".to_string(),
                "Status".to_string(),
            ],
            vec![
                Constraint::Length(7),
                Constraint::Min(22),
                Constraint::Length(13),
                Constraint::Length(10),
                Constraint::Length(8),
                Constraint::Length(10),
                Constraint::Length(10),
            ],
        ),
        Tab::Energy => (
            vec![
                sort_header("PID", sort_key == crate::app::SortKey::Pid, sort_desc),
                sort_header("Process", sort_key == crate::app::SortKey::Name, sort_desc),
                sort_header("User", sort_key == crate::app::SortKey::User, sort_desc),
                sort_header("Impact", sort_key == crate::app::SortKey::Energy, sort_desc),
                sort_header("% CPU", sort_key == crate::app::SortKey::Cpu, sort_desc),
                "Disk/s".to_string(),
                "Status".to_string(),
            ],
            vec![
                Constraint::Length(7),
                Constraint::Min(22),
                Constraint::Length(13),
                Constraint::Length(8),
                Constraint::Length(8),
                Constraint::Length(10),
                Constraint::Length(10),
            ],
        ),
        Tab::Disk => (
            vec![
                sort_header("PID", sort_key == crate::app::SortKey::Pid, sort_desc),
                sort_header("Process", sort_key == crate::app::SortKey::Name, sort_desc),
                sort_header("User", sort_key == crate::app::SortKey::User, sort_desc),
                sort_header(
                    "Read/s",
                    sort_key == crate::app::SortKey::DiskRead,
                    sort_desc,
                ),
                sort_header(
                    "Write/s",
                    sort_key == crate::app::SortKey::DiskWrite,
                    sort_desc,
                ),
                "Read".to_string(),
                "Written".to_string(),
            ],
            vec![
                Constraint::Length(7),
                Constraint::Min(22),
                Constraint::Length(13),
                Constraint::Length(10),
                Constraint::Length(10),
                Constraint::Length(10),
                Constraint::Length(10),
            ],
        ),
        Tab::Network => (
            vec![
                sort_header("PID", sort_key == crate::app::SortKey::Pid, sort_desc),
                sort_header("Process", sort_key == crate::app::SortKey::Name, sort_desc),
                sort_header("User", sort_key == crate::app::SortKey::User, sort_desc),
                sort_header("% CPU", sort_key == crate::app::SortKey::Cpu, sort_desc),
                sort_header("Memory", sort_key == crate::app::SortKey::Memory, sort_desc),
                "Disk/s".to_string(),
                "Status".to_string(),
            ],
            vec![
                Constraint::Length(7),
                Constraint::Min(22),
                Constraint::Length(13),
                Constraint::Length(8),
                Constraint::Length(10),
                Constraint::Length(10),
                Constraint::Length(10),
            ],
        ),
    };
    (headers, widths)
}

fn process_row(process: &ProcessRow, tab: Tab) -> Row<'static> {
    let pid = right(process.pid.to_string());
    let name = format::truncate_middle(&process.name, 64);
    let user = process.user.clone();
    let status = process.status.clone();
    let disk_rate = process.disk_read_rate + process.disk_write_rate;
    let cells = match tab {
        Tab::Cpu => vec![
            pid,
            name,
            user,
            right(format::percent(process.cpu_usage as f64)),
            right(format::bytes(process.memory)),
            right(format::duration(process.run_time)),
            status,
        ],
        Tab::Memory => vec![
            pid,
            name,
            user,
            right(format::bytes(process.memory)),
            right(format::percent(process.memory_percent)),
            right(format::bytes(process.virtual_memory)),
            status,
        ],
        Tab::Energy => vec![
            pid,
            name,
            user,
            right(format::number(process.energy_impact)),
            right(format::percent(process.cpu_usage as f64)),
            right(format::bytes_rate(disk_rate)),
            status,
        ],
        Tab::Disk => vec![
            pid,
            name,
            user,
            right(format::bytes_rate(process.disk_read_rate)),
            right(format::bytes_rate(process.disk_write_rate)),
            right(format::bytes(process.total_disk_read)),
            right(format::bytes(process.total_disk_write)),
        ],
        Tab::Network => vec![
            pid,
            name,
            user,
            right(format::percent(process.cpu_usage as f64)),
            right(format::bytes(process.memory)),
            right(format::bytes_rate(disk_rate)),
            status,
        ],
    };

    Row::new(cells.into_iter().map(Cell::from).collect::<Vec<_>>())
        .style(process_style(process))
        .height(1)
}

fn render_details(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let selected = app.selected_process();
    let mut lines = Vec::new();

    if let Some(process) = selected {
        push_pair(&mut lines, "Name", &process.name);
        push_pair(&mut lines, "PID", &process.pid.to_string());
        push_pair(
            &mut lines,
            "Parent",
            &process
                .parent_pid
                .map(|pid| pid.to_string())
                .unwrap_or_else(|| "-".to_string()),
        );
        push_pair(&mut lines, "User", &process.user);
        push_pair(&mut lines, "Status", &process.status);
        push_pair(
            &mut lines,
            "CPU",
            &format::percent(process.cpu_usage as f64),
        );
        push_pair(&mut lines, "Memory", &format::bytes(process.memory));
        push_pair(
            &mut lines,
            "Virtual",
            &format::bytes(process.virtual_memory),
        );
        push_pair(&mut lines, "Runtime", &format::duration(process.run_time));
        push_pair(
            &mut lines,
            "Started",
            &format::epoch_time(process.start_time),
        );
        push_pair(
            &mut lines,
            "Disk",
            &format!(
                "R {} W {}",
                format::bytes_rate(process.disk_read_rate),
                format::bytes_rate(process.disk_write_rate)
            ),
        );
        push_pair(
            &mut lines,
            "Total I/O",
            &format!(
                "R {} W {}",
                format::bytes(process.total_disk_read),
                format::bytes(process.total_disk_write)
            ),
        );
        push_pair(&mut lines, "Impact", &format::number(process.energy_impact));
        if let Some(details) = &process.selected_details {
            push_pair(
                &mut lines,
                "Threads",
                &details
                    .thread_count
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".to_string()),
            );
            push_pair(
                &mut lines,
                "Open files",
                &details
                    .open_files
                    .map(|files| {
                        details
                            .open_files_limit
                            .map(|limit| format!("{files}/{limit}"))
                            .unwrap_or_else(|| files.to_string())
                    })
                    .unwrap_or_else(|| "-".to_string()),
            );
            push_pair(
                &mut lines,
                "Session",
                &details
                    .session_id
                    .map(|pid| pid.to_string())
                    .unwrap_or_else(|| "-".to_string()),
            );
        }
        lines.push(Line::from(""));
        push_pair(&mut lines, "Exe", &process.exe);
        push_pair(&mut lines, "CWD", &process.cwd);
        push_pair(&mut lines, "Command", &process.command);
    } else {
        let empty_message = if app.visible_count() == 0 && !app.filter.is_empty() {
            "No processes match the current filter"
        } else {
            "No process selected"
        };
        lines.push(Line::from(Span::styled(
            empty_message,
            Style::default().fg(MUTED),
        )));
        if app.visible_count() == 0 && !app.filter.is_empty() {
            lines.push(Line::from(Span::styled(
                "Press Ctrl-U to clear the filter or / to refine it.",
                Style::default().fg(MUTED),
            )));
        }
    }

    if app.tab == Tab::Disk {
        append_disk_lines(&mut lines, &app.snapshot().disks);
    }
    if app.tab == Tab::Network {
        append_network_lines(&mut lines, &app.snapshot().networks);
    }

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .title(inspector_title(app))
                .borders(Borders::ALL)
                .style(panel_style()),
        )
        .style(Style::default().fg(TEXT))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn inspector_title(app: &App) -> String {
    let pid = app.selected_pid().map(|pid| format!("pid {pid}"));
    match app.tab {
        Tab::Disk => pid
            .map(|pid| format!("Inspector | {pid} + volumes"))
            .unwrap_or_else(|| "Inspector | volumes".to_string()),
        Tab::Network => pid
            .map(|pid| format!("Inspector | {pid} + interfaces"))
            .unwrap_or_else(|| "Inspector | interfaces".to_string()),
        _ => pid
            .map(|pid| format!("Inspector | {pid}"))
            .unwrap_or_else(|| "Inspector".to_string()),
    }
}

fn append_disk_lines(lines: &mut Vec<Line<'static>>, disks: &[DiskRow]) {
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Volumes",
        Style::default().fg(GREEN).add_modifier(Modifier::BOLD),
    )));
    for disk in disks.iter().take(4) {
        lines.push(Line::from(vec![
            Span::styled(format!("{} ", disk.name), Style::default().fg(TEXT)),
            Span::styled(
                format!(
                    "{} free of {}, R {}, W {}",
                    format::bytes(disk.available),
                    format::bytes(disk.total),
                    format::bytes_rate(disk.read_rate),
                    format::bytes_rate(disk.write_rate)
                ),
                Style::default().fg(MUTED),
            ),
        ]));
        lines.push(Line::from(Span::styled(
            disk.mount_point.clone(),
            Style::default().fg(MUTED),
        )));
    }
}

fn append_network_lines(lines: &mut Vec<Line<'static>>, networks: &[NetworkRow]) {
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Interfaces",
        Style::default().fg(GREEN).add_modifier(Modifier::BOLD),
    )));
    for network in networks.iter().take(5) {
        lines.push(Line::from(vec![
            Span::styled(format!("{} ", network.name), Style::default().fg(TEXT)),
            Span::styled(
                format!(
                    "in {}, out {}",
                    format::bytes_rate(network.received_rate),
                    format::bytes_rate(network.transmitted_rate),
                ),
                Style::default().fg(MUTED),
            ),
        ]));
        lines.push(Line::from(Span::styled(
            format!(
                "total in {}, total out {}",
                format::bytes(network.total_received),
                format::bytes(network.total_transmitted)
            ),
            Style::default().fg(MUTED),
        )));
    }
}

fn render_footer(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let mode = if app.filter_mode {
        if area.width >= 90 {
            "filter: type to search, Backspace edits, Enter or Esc keeps focus, Ctrl-U clears"
        } else {
            "filter: type, Backspace edit, Enter/Esc keep, Ctrl-U clear"
        }
    } else if area.width >= 150 {
        "1-5/Tab views  j/k move  PgUp/PgDn jump  / filter  Ctrl-U clear  s cycle  S reverse  i inspector  +/- refresh  r resample  ? help  q quit"
    } else if area.width >= 110 {
        "1-5 views  j/k move  / filter  Ctrl-U clear  s/S sort  i inspector  +/- refresh  ? help  q quit"
    } else {
        "1-5 views  j/k move  / filter  s/S sort  i info  ? help  q quit"
    };
    let footer = Paragraph::new(Line::from(vec![
        Span::styled(" ", Style::default().bg(PANEL)),
        Span::styled(mode, Style::default().fg(MUTED).bg(PANEL)),
    ]))
    .style(Style::default().bg(PANEL));
    frame.render_widget(footer, area);
}

fn render_help(frame: &mut Frame<'_>, area: Rect) {
    let popup = centered_rect(90, 88, area);
    frame.render_widget(Clear, popup);
    let lines = vec![
        Line::from(Span::styled(
            "monitr controls",
            Style::default().fg(GREEN).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("1-5 / Tab        switch category"),
        Line::from("j/k or arrows    move process selection"),
        Line::from("Page/Home/End    jump through the process table"),
        Line::from("/                filter by name, pid, user, command, or status"),
        Line::from("Ctrl-U           clear the active filter anywhere"),
        Line::from("s / S            cycle sort key / reverse sort"),
        Line::from(
            "c m e d D n p T u sort CPU, memory, impact, write, read, name, pid, runtime, user",
        ),
        Line::from("i or Enter       show or hide process inspector"),
        Line::from("t / f            send TERM / KILL after confirmation"),
        Line::from("+ / -            slower / faster refresh interval"),
        Line::from("r                refresh immediately"),
        Line::from("q, Esc, Ctrl-C  quit"),
        Line::from(""),
        Line::from("Disk and Network inspector panels show system-level volumes/interfaces."),
        Line::from(""),
        Line::from("Press Esc, Enter, ?, or q to close."),
    ];
    let help = Paragraph::new(lines)
        .block(
            Block::default()
                .title("Help")
                .borders(Borders::ALL)
                .style(panel_style()),
        )
        .style(Style::default().fg(TEXT))
        .wrap(Wrap { trim: true });
    frame.render_widget(help, popup);
}

fn push_pair(lines: &mut Vec<Line<'static>>, label: &str, value: &str) {
    lines.push(Line::from(vec![
        Span::styled(format!("{label:<10}"), Style::default().fg(MUTED)),
        Span::styled(value.to_string(), Style::default().fg(TEXT)),
    ]));
}

fn right(value: String) -> String {
    value
}

fn process_style(process: &ProcessRow) -> Style {
    let fg = if process.status == "zombie" || process.status == "dead" {
        RED
    } else if process.cpu_usage >= 75.0 {
        YELLOW
    } else {
        TEXT
    };
    Style::default().fg(fg).bg(BG)
}

fn panel_style() -> Style {
    Style::default().fg(TEXT).bg(PANEL)
}

fn metric_block(title: &'static str) -> Block<'static> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .style(panel_style())
}

fn value_style(color: Color) -> Style {
    Style::default().fg(color).add_modifier(Modifier::BOLD)
}

fn sort_header(label: &str, active: bool, descending: bool) -> String {
    if active {
        format!("{label} {}", if descending { "v" } else { "^" })
    } else {
        label.to_string()
    }
}

fn usage_color(value: f64) -> Color {
    if value >= 85.0 {
        RED
    } else if value >= 60.0 {
        YELLOW
    } else {
        GREEN
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1]);
    horizontal[1]
}
