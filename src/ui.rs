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
    let status = app
        .confirm
        .map(|intent| {
            let pid = app
                .selected_pid()
                .map(|pid| pid.to_string())
                .unwrap_or_else(|| "-".to_string());
            format!("Confirm {} pid {}: y/n", intent.label(), pid)
        })
        .or_else(|| app.notice.as_ref().map(|notice| notice.text().to_string()))
        .unwrap_or_else(|| {
            format!(
                "{} processes | sort {} {} | refresh {} ms | sample {:.2}s | uptime {}",
                snapshot.process_count,
                app.sort_key.title(),
                if app.sort_desc { "desc" } else { "asc" },
                app.interval().as_millis(),
                snapshot.sample_span.as_secs_f64(),
                format::duration(snapshot.totals.uptime)
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
        Span::styled(status, Style::default().fg(YELLOW)),
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
    let (headers, widths) = table_schema(app.tab);
    let header = Row::new(
        headers
            .iter()
            .map(|header| Cell::from(*header))
            .collect::<Vec<_>>(),
    )
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

    let title = format!("{} | {} visible", app.tab.title(), app.visible.len());
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

fn render_stateful_table(
    frame: &mut Frame<'_>,
    table: Table<'_>,
    area: Rect,
    state: &mut TableState,
) {
    frame.render_stateful_widget(table, area, state);
}

fn table_schema(tab: Tab) -> (Vec<&'static str>, Vec<Constraint>) {
    match tab {
        Tab::Cpu => (
            vec![
                "PID", "Process", "User", "% CPU", "Memory", "Time", "Status",
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
                "PID", "Process", "User", "Memory", "% Mem", "Virtual", "Status",
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
                "PID", "Process", "User", "Impact", "% CPU", "Disk/s", "Status",
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
                "PID", "Process", "User", "Read/s", "Write/s", "Read", "Written",
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
                "PID", "Process", "User", "% CPU", "Memory", "Disk/s", "Status",
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
    }
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
        lines.push(Line::from(Span::styled(
            "No process selected",
            Style::default().fg(MUTED),
        )));
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
                .title("Inspector")
                .borders(Borders::ALL)
                .style(panel_style()),
        )
        .style(Style::default().fg(TEXT))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
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
        "filter mode: type, enter to keep, ctrl-u clear"
    } else {
        "1-5 tabs  j/k move  / filter  s sort  S reverse  i details  t term  f kill  +/- interval  ? help  q quit"
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
        Line::from("/                filter by name, pid, user, command, status"),
        Line::from("s / S            cycle sort key / reverse sort"),
        Line::from("c m e d n p u    sort CPU, memory, impact, disk, name, pid, user"),
        Line::from("i or Enter       show or hide process inspector"),
        Line::from("t / f            send TERM / KILL after confirmation"),
        Line::from("+ / -            slower or faster refresh interval"),
        Line::from("r                refresh immediately"),
        Line::from("q or Esc         quit"),
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
