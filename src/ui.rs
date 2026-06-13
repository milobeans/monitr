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
    paragraph::{Paragraph, Wrap},
    table::{Cell, Row, Table, TableState},
    tabs::Tabs,
};

use crate::{
    app::{App, HandlesView, Tab},
    format,
    sampler::{DiskRow, NetworkRow, ProcessRow},
};

const BG: Color = Color::Rgb(7, 10, 16);
const PANEL: Color = Color::Rgb(16, 21, 33);
const PANEL_ALT: Color = Color::Rgb(24, 30, 47);
const TEXT: Color = Color::Rgb(235, 241, 255);
const MUTED: Color = Color::Rgb(142, 156, 190);
const GREEN: Color = Color::Rgb(46, 224, 140);
const BLUE: Color = Color::Rgb(66, 165, 255);
const YELLOW: Color = Color::Rgb(255, 198, 46);
const RED: Color = Color::Rgb(255, 90, 106);
const SUMMARY_GREEN: Color = Color::Rgb(41, 118, 84);
const SUMMARY_YELLOW: Color = Color::Rgb(173, 145, 45);
const SUMMARY_RED: Color = Color::Rgb(163, 69, 57);
const COMPACT_TABLE_WIDTH: u16 = 96;

pub fn draw(frame: &mut Frame<'_>, app: &mut App) {
    let area = frame.area();
    frame.render_widget(Block::default().style(Style::default().bg(BG)), area);

    let overview_height = if app.overview_visible {
        Constraint::Length(7)
    } else {
        Constraint::Length(0)
    };

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(3),
            overview_height,
            Constraint::Min(8),
            Constraint::Length(1),
        ])
        .split(area);

    render_title(frame, app, layout[0]);
    render_tabs(frame, app, layout[1]);
    if app.overview_visible {
        render_overview(frame, app, layout[2]);
    }
    render_main(frame, app, layout[3]);
    render_footer(frame, app, layout[4]);

    if app.show_help {
        render_help(frame, area, app);
    }
    if let Some(view) = app.handles_view() {
        render_handles(frame, area, view);
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
    let snapshot = app.snapshot();
    let titles = Tab::ALL
        .iter()
        .map(|tab| {
            let label = match tab {
                Tab::Cpu => format!(
                    "CPU ({})",
                    format::percent(snapshot.totals.cpu_usage as f64)
                ),
                Tab::Memory => format!("Mem ({})", format::bytes(snapshot.totals.used_memory)),
                Tab::Energy => "Energy".to_string(),
                Tab::Disk => format!(
                    "Disk (R{} W{})",
                    format::bytes_rate(snapshot.totals.disk_read_rate),
                    format::bytes_rate(snapshot.totals.disk_write_rate)
                ),
                Tab::Network => format!(
                    "Net ({} / {})",
                    format::bytes_rate(snapshot.totals.net_in_rate),
                    format::bytes_rate(snapshot.totals.net_out_rate)
                ),
                Tab::Movers => "Movers".to_string(),
            };
            Line::from(label)
        })
        .collect::<Vec<_>>();
    let selected = Tab::ALL.iter().position(|tab| *tab == app.tab).unwrap_or(0);
    let tabs = Tabs::new(titles)
        .select(selected)
        .block(Block::default().borders(Borders::ALL).style(panel_style()))
        .style(Style::default().fg(MUTED))
        .highlight_style(
            Style::default()
                .fg(GREEN)
                .bg(PANEL_ALT)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        );
    frame.render_widget(tabs, area);
}

fn render_overview(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let snapshot = app.snapshot();
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ])
        .split(area);

    render_usage_chart(
        frame,
        chunks[0],
        "CPU",
        &app.history().cpu_recent(chart_width(chunks[0]) * 2),
        100.0,
        snapshot.totals.cpu_usage as f64,
        &[
            format::percent(snapshot.totals.cpu_usage as f64),
            "across".to_string(),
            format!("{} Cores", snapshot.totals.cpu_count),
        ],
    );

    let memory_ratio = if snapshot.totals.total_memory > 0 {
        snapshot.totals.used_memory as f64 / snapshot.totals.total_memory as f64
    } else {
        0.0
    };
    render_usage_chart(
        frame,
        chunks[1],
        "Memory",
        &app.history().memory_recent(chart_width(chunks[1]) * 2),
        100.0,
        memory_ratio * 100.0,
        &[
            format::percent(memory_ratio * 100.0),
            format!(
                "{}/{}",
                format::bytes(snapshot.totals.used_memory),
                format::bytes(snapshot.totals.total_memory)
            ),
            format!(
                "swap {}/{}",
                format::bytes(snapshot.totals.used_swap),
                format::bytes(snapshot.totals.total_swap)
            ),
        ],
    );

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
    app.table_area = area;

    let compact = is_compact(app.compact_mode, area.width);
    let (headers, widths) = table_schema_for_width(app, area.width);
    let header = Row::new(headers.iter().cloned().map(Cell::from).collect::<Vec<_>>())
        .style(
            Style::default()
                .fg(TEXT)
                .bg(PANEL_ALT)
                .add_modifier(Modifier::BOLD),
        )
        .height(1);

    let view = process_table_view(
        app.visible.len(),
        app.table_state.selected(),
        app.table_state.offset(),
        process_table_body_capacity(area),
    );
    *app.table_state.offset_mut() = view.start;
    let mut viewport_state = TableState::default()
        .with_offset(0)
        .with_selected(view.selected);

    let rows = app
        .visible
        .iter()
        .skip(view.start)
        .take(view.end.saturating_sub(view.start))
        .filter_map(|index| app.snapshot().processes.get(*index))
        .map(|process| process_row(process, app.tab, compact))
        .collect::<Vec<_>>();

    let title = process_table_title(app);
    let no_match = app.visible_count() == 0 && !app.filter.is_empty();
    let border_color = if no_match { RED } else { GREEN };
    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .title(
                    Line::from(format!(" {title} ")).style(
                        Style::default()
                            .fg(border_color)
                            .add_modifier(Modifier::BOLD),
                    ),
                )
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color))
                .style(panel_style()),
        )
        .row_highlight_style(
            Style::default()
                .fg(Color::White)
                .bg(Color::Rgb(51, 74, 94))
                .add_modifier(Modifier::BOLD),
        )
        .column_spacing(1);
    render_stateful_table(frame, table, area, &mut viewport_state);
}

fn process_table_title(app: &App) -> String {
    let base = match app.tab {
        Tab::Disk => "Disk activity by process".to_string(),
        Tab::Network => "Process context | interface totals in inspector".to_string(),
        Tab::Movers => "Top movers since last sample".to_string(),
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

fn process_table_body_capacity(area: Rect) -> usize {
    area.height.saturating_sub(3) as usize
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ProcessTableView {
    start: usize,
    end: usize,
    selected: Option<usize>,
}

fn process_table_view(
    visible_len: usize,
    selected: Option<usize>,
    offset: usize,
    body_capacity: usize,
) -> ProcessTableView {
    if visible_len == 0 || body_capacity == 0 {
        return ProcessTableView {
            start: 0,
            end: 0,
            selected: None,
        };
    }

    let max_start = visible_len.saturating_sub(body_capacity);
    let mut start = offset.min(max_start);
    let selected = selected.map(|selected| selected.min(visible_len - 1));

    if let Some(selected) = selected {
        if selected < start {
            start = selected;
        } else if selected >= start + body_capacity {
            start = selected + 1 - body_capacity;
        }
    }

    let end = (start + body_capacity).min(visible_len);
    let selected = selected.map(|selected| selected.saturating_sub(start));

    ProcessTableView {
        start,
        end,
        selected,
    }
}

fn table_schema_for_width(app: &App, area_width: u16) -> (Vec<String>, Vec<Constraint>) {
    table_schema_for_fields(
        app.tab,
        app.sort_key,
        app.sort_desc,
        app.compact_mode,
        area_width,
    )
}

fn table_schema_for_fields(
    tab: Tab,
    sort_key: crate::app::SortKey,
    sort_desc: bool,
    compact_mode: bool,
    area_width: u16,
) -> (Vec<String>, Vec<Constraint>) {
    if is_compact(compact_mode, area_width) {
        return compact_table_schema(tab, sort_key, sort_desc);
    }

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
                sort_header(
                    "Net In",
                    sort_key == crate::app::SortKey::NetworkIn,
                    sort_desc,
                ),
                sort_header(
                    "Net Out",
                    sort_key == crate::app::SortKey::NetworkOut,
                    sort_desc,
                ),
                sort_header("% CPU", sort_key == crate::app::SortKey::Cpu, sort_desc),
                "Status".to_string(),
            ],
            vec![
                Constraint::Length(7),
                Constraint::Min(22),
                Constraint::Length(13),
                Constraint::Length(10),
                Constraint::Length(10),
                Constraint::Length(8),
                Constraint::Length(10),
            ],
        ),
        Tab::Movers => (
            vec![
                sort_header("PID", sort_key == crate::app::SortKey::Pid, sort_desc),
                sort_header("Process", sort_key == crate::app::SortKey::Name, sort_desc),
                sort_header("User", sort_key == crate::app::SortKey::User, sort_desc),
                sort_header("CPU +/-", sort_key == crate::app::SortKey::Trend, sort_desc),
                "Mem +/-".to_string(),
                "Disk +/-".to_string(),
                "State".to_string(),
            ],
            vec![
                Constraint::Length(7),
                Constraint::Min(22),
                Constraint::Length(13),
                Constraint::Length(9),
                Constraint::Length(10),
                Constraint::Length(11),
                Constraint::Length(10),
            ],
        ),
    };
    (headers, widths)
}

pub fn column_widths(
    tab: Tab,
    sort_key: crate::app::SortKey,
    sort_desc: bool,
    compact_mode: bool,
    area_width: u16,
) -> Vec<usize> {
    let (_, constraints) =
        table_schema_for_fields(tab, sort_key, sort_desc, compact_mode, area_width);
    let inner_width = area_width.saturating_sub(2) as usize;
    let spacing = constraints.len().saturating_sub(1);
    let fixed: usize = constraints
        .iter()
        .filter_map(|c| match c {
            Constraint::Length(n) => Some(*n as usize),
            _ => None,
        })
        .sum();
    let min_total: usize = constraints
        .iter()
        .filter_map(|c| match c {
            Constraint::Min(n) => Some(*n as usize),
            _ => None,
        })
        .sum();
    let available = inner_width.saturating_sub(fixed + spacing);
    let min_count = constraints
        .iter()
        .filter(|c| matches!(c, Constraint::Min(_)))
        .count();

    constraints
        .iter()
        .map(|c| match c {
            Constraint::Length(n) => *n as usize,
            Constraint::Min(n) => {
                let extra = available.saturating_sub(min_total);
                let share = extra.checked_div(min_count).unwrap_or(0);
                *n as usize + share
            }
            _ => 0,
        })
        .collect()
}

pub fn column_sort_key(
    tab: Tab,
    compact_mode: bool,
    index: usize,
    area_width: u16,
) -> Option<crate::app::SortKey> {
    if is_compact(compact_mode, area_width) {
        return compact_column_sort_key(tab, index);
    }
    full_column_sort_key(tab, index)
}

fn process_row(process: &ProcessRow, tab: Tab, compact: bool) -> Row<'static> {
    let pid = right(process.pid.to_string(), 7);
    let name = format::truncate_middle(&process.name, 64);
    let user = process.user.clone();
    let status = process.status.clone();
    let disk_rate = process.disk_read_rate + process.disk_write_rate;

    let cpu_cell = colored_value_cell(
        &format::percent(process.cpu_usage as f64),
        process.cpu_usage as f64,
        8,
    );
    let memory_cell =
        colored_value_cell(&format::bytes(process.memory), process.memory_percent, 10);

    let cpu_trend = trend_arrow(process.trend.cpu_delta as f64);
    let mem_trend = trend_arrow(process.trend.memory_delta as f64);
    let network_in = process
        .network_in_rate
        .map(format::bytes_rate)
        .unwrap_or_else(|| "-".to_string());
    let network_out = process
        .network_out_rate
        .map(format::bytes_rate)
        .unwrap_or_else(|| "-".to_string());

    let cells = if compact {
        compact_process_cells(process, tab, &pid, &name, &status)
    } else {
        match tab {
            Tab::Cpu => vec![
                Cell::from(pid),
                Cell::from(name),
                Cell::from(user),
                Cell::from(format!(
                    "{}{}",
                    right(format::percent(process.cpu_usage as f64), 7),
                    cpu_trend
                ))
                .style(Style::default().fg(usage_color(process.cpu_usage as f64))),
                Cell::from(format!(
                    "{}{}",
                    right(format::bytes(process.memory), 9),
                    mem_trend
                )),
                Cell::from(right(format::duration(process.run_time), 9)),
                Cell::from(status),
            ],
            Tab::Memory => vec![
                Cell::from(pid),
                Cell::from(name),
                Cell::from(user),
                memory_cell,
                Cell::from(right(format::percent(process.memory_percent), 7)),
                Cell::from(right(format::bytes(process.virtual_memory), 9)),
                Cell::from(status),
            ],
            Tab::Energy => vec![
                Cell::from(pid),
                Cell::from(name),
                Cell::from(user),
                Cell::from(right(format::number(process.energy_impact), 7)),
                cpu_cell,
                Cell::from(right(format::bytes_rate(disk_rate), 9)),
                Cell::from(status),
            ],
            Tab::Disk => vec![
                Cell::from(pid),
                Cell::from(name),
                Cell::from(user),
                Cell::from(right(format::bytes_rate(process.disk_read_rate), 9)),
                Cell::from(right(format::bytes_rate(process.disk_write_rate), 9)),
                Cell::from(right(format::bytes(process.total_disk_read), 9)),
                Cell::from(right(format::bytes(process.total_disk_write), 9)),
            ],
            Tab::Network => vec![
                Cell::from(pid),
                Cell::from(name),
                Cell::from(user),
                Cell::from(right(network_in, 9)),
                Cell::from(right(network_out, 9)),
                cpu_cell,
                Cell::from(status),
            ],
            Tab::Movers => vec![
                Cell::from(pid),
                Cell::from(name),
                Cell::from(user),
                Cell::from(right(
                    format::signed_percent(process.trend.cpu_delta as f64),
                    8,
                )),
                Cell::from(right(format::signed_bytes(process.trend.memory_delta), 9)),
                Cell::from(right(
                    format::signed_bytes_rate(process.trend.disk_rate_delta()),
                    10,
                )),
                Cell::from(process.trend.headline().unwrap_or(status)),
            ],
        }
    };

    Row::new(cells).style(process_style(process)).height(1)
}

fn compact_process_cells(
    process: &ProcessRow,
    tab: Tab,
    pid: &str,
    name: &str,
    status: &str,
) -> Vec<Cell<'static>> {
    match tab {
        Tab::Cpu => vec![
            Cell::from(pid.to_string()),
            Cell::from(name.to_string()),
            Cell::from(right(format::percent(process.cpu_usage as f64), 7))
                .style(Style::default().fg(usage_color(process.cpu_usage as f64))),
            Cell::from(right(format::bytes(process.memory), 8)),
            Cell::from(status.to_string()),
        ],
        Tab::Memory => vec![
            Cell::from(pid.to_string()),
            Cell::from(name.to_string()),
            Cell::from(right(format::bytes(process.memory), 8)),
            Cell::from(right(format::percent(process.memory_percent), 6)),
            Cell::from(status.to_string()),
        ],
        Tab::Energy => vec![
            Cell::from(pid.to_string()),
            Cell::from(name.to_string()),
            Cell::from(right(format::number(process.energy_impact), 7)),
            Cell::from(right(format::percent(process.cpu_usage as f64), 7)),
            Cell::from(status.to_string()),
        ],
        Tab::Disk => vec![
            Cell::from(pid.to_string()),
            Cell::from(name.to_string()),
            Cell::from(right(format::bytes_rate(process.disk_read_rate), 8)),
            Cell::from(right(format::bytes_rate(process.disk_write_rate), 8)),
        ],
        Tab::Network => vec![
            Cell::from(pid.to_string()),
            Cell::from(name.to_string()),
            Cell::from(right(
                process
                    .network_in_rate
                    .map(format::bytes_rate)
                    .unwrap_or_else(|| "-".to_string()),
                9,
            )),
            Cell::from(right(
                process
                    .network_out_rate
                    .map(format::bytes_rate)
                    .unwrap_or_else(|| "-".to_string()),
                9,
            )),
            Cell::from(status.to_string()),
        ],
        Tab::Movers => vec![
            Cell::from(pid.to_string()),
            Cell::from(name.to_string()),
            Cell::from(right(
                format::signed_percent(process.trend.cpu_delta as f64),
                7,
            )),
            Cell::from(right(format::signed_bytes(process.trend.memory_delta), 8)),
            Cell::from(
                process
                    .trend
                    .headline()
                    .unwrap_or_else(|| status.to_string()),
            ),
        ],
    }
}

fn compact_table_schema(
    tab: Tab,
    sort_key: crate::app::SortKey,
    sort_desc: bool,
) -> (Vec<String>, Vec<Constraint>) {
    match tab {
        Tab::Cpu => (
            vec![
                sort_header("PID", sort_key == crate::app::SortKey::Pid, sort_desc),
                sort_header("Process", sort_key == crate::app::SortKey::Name, sort_desc),
                sort_header("% CPU", sort_key == crate::app::SortKey::Cpu, sort_desc),
                sort_header("Mem", sort_key == crate::app::SortKey::Memory, sort_desc),
                "State".to_string(),
            ],
            compact_widths(&[6, 0, 9, 9, 8]),
        ),
        Tab::Memory => (
            vec![
                sort_header("PID", sort_key == crate::app::SortKey::Pid, sort_desc),
                sort_header("Process", sort_key == crate::app::SortKey::Name, sort_desc),
                sort_header("Mem", sort_key == crate::app::SortKey::Memory, sort_desc),
                "%Mem".to_string(),
                "State".to_string(),
            ],
            compact_widths(&[6, 0, 9, 7, 8]),
        ),
        Tab::Energy => (
            vec![
                sort_header("PID", sort_key == crate::app::SortKey::Pid, sort_desc),
                sort_header("Process", sort_key == crate::app::SortKey::Name, sort_desc),
                sort_header("Impact", sort_key == crate::app::SortKey::Energy, sort_desc),
                sort_header("% CPU", sort_key == crate::app::SortKey::Cpu, sort_desc),
                "State".to_string(),
            ],
            compact_widths(&[6, 0, 8, 8, 8]),
        ),
        Tab::Disk => (
            vec![
                sort_header("PID", sort_key == crate::app::SortKey::Pid, sort_desc),
                sort_header("Process", sort_key == crate::app::SortKey::Name, sort_desc),
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
            ],
            compact_widths(&[6, 0, 9, 9]),
        ),
        Tab::Network => (
            vec![
                sort_header("PID", sort_key == crate::app::SortKey::Pid, sort_desc),
                sort_header("Process", sort_key == crate::app::SortKey::Name, sort_desc),
                sort_header(
                    "Net In",
                    sort_key == crate::app::SortKey::NetworkIn,
                    sort_desc,
                ),
                sort_header(
                    "Net Out",
                    sort_key == crate::app::SortKey::NetworkOut,
                    sort_desc,
                ),
                "State".to_string(),
            ],
            compact_widths(&[6, 0, 9, 9, 8]),
        ),
        Tab::Movers => (
            vec![
                sort_header("PID", sort_key == crate::app::SortKey::Pid, sort_desc),
                sort_header("Process", sort_key == crate::app::SortKey::Name, sort_desc),
                sort_header("CPU +/-", sort_key == crate::app::SortKey::Trend, sort_desc),
                "Mem +/-".to_string(),
                "State".to_string(),
            ],
            compact_widths(&[6, 0, 8, 9, 10]),
        ),
    }
}

fn compact_widths(widths: &[u16]) -> Vec<Constraint> {
    widths
        .iter()
        .map(|width| {
            if *width == 0 {
                Constraint::Min(12)
            } else {
                Constraint::Length(*width)
            }
        })
        .collect()
}

fn is_compact(compact_mode: bool, area_width: u16) -> bool {
    compact_mode || area_width < COMPACT_TABLE_WIDTH
}

fn compact_column_sort_key(tab: Tab, index: usize) -> Option<crate::app::SortKey> {
    match tab {
        Tab::Cpu => match index {
            0 => Some(crate::app::SortKey::Pid),
            1 => Some(crate::app::SortKey::Name),
            2 => Some(crate::app::SortKey::Cpu),
            3 => Some(crate::app::SortKey::Memory),
            _ => None,
        },
        Tab::Memory => match index {
            0 => Some(crate::app::SortKey::Pid),
            1 => Some(crate::app::SortKey::Name),
            2 => Some(crate::app::SortKey::Memory),
            _ => None,
        },
        Tab::Energy => match index {
            0 => Some(crate::app::SortKey::Pid),
            1 => Some(crate::app::SortKey::Name),
            2 => Some(crate::app::SortKey::Energy),
            3 => Some(crate::app::SortKey::Cpu),
            _ => None,
        },
        Tab::Disk => match index {
            0 => Some(crate::app::SortKey::Pid),
            1 => Some(crate::app::SortKey::Name),
            2 => Some(crate::app::SortKey::DiskRead),
            3 => Some(crate::app::SortKey::DiskWrite),
            _ => None,
        },
        Tab::Network => match index {
            0 => Some(crate::app::SortKey::Pid),
            1 => Some(crate::app::SortKey::Name),
            2 => Some(crate::app::SortKey::NetworkIn),
            3 => Some(crate::app::SortKey::NetworkOut),
            _ => None,
        },
        Tab::Movers => match index {
            0 => Some(crate::app::SortKey::Pid),
            1 => Some(crate::app::SortKey::Name),
            2 => Some(crate::app::SortKey::Trend),
            _ => None,
        },
    }
}

fn full_column_sort_key(tab: Tab, index: usize) -> Option<crate::app::SortKey> {
    match tab {
        Tab::Cpu => match index {
            0 => Some(crate::app::SortKey::Pid),
            1 => Some(crate::app::SortKey::Name),
            2 => Some(crate::app::SortKey::User),
            3 => Some(crate::app::SortKey::Cpu),
            4 => Some(crate::app::SortKey::Memory),
            5 => Some(crate::app::SortKey::Runtime),
            _ => None,
        },
        Tab::Memory => match index {
            0 => Some(crate::app::SortKey::Pid),
            1 => Some(crate::app::SortKey::Name),
            2 => Some(crate::app::SortKey::User),
            3 => Some(crate::app::SortKey::Memory),
            _ => None,
        },
        Tab::Energy => match index {
            0 => Some(crate::app::SortKey::Pid),
            1 => Some(crate::app::SortKey::Name),
            2 => Some(crate::app::SortKey::User),
            3 => Some(crate::app::SortKey::Energy),
            4 => Some(crate::app::SortKey::Cpu),
            _ => None,
        },
        Tab::Disk => match index {
            0 => Some(crate::app::SortKey::Pid),
            1 => Some(crate::app::SortKey::Name),
            2 => Some(crate::app::SortKey::User),
            3 => Some(crate::app::SortKey::DiskRead),
            4 => Some(crate::app::SortKey::DiskWrite),
            _ => None,
        },
        Tab::Network => match index {
            0 => Some(crate::app::SortKey::Pid),
            1 => Some(crate::app::SortKey::Name),
            2 => Some(crate::app::SortKey::User),
            3 => Some(crate::app::SortKey::NetworkIn),
            4 => Some(crate::app::SortKey::NetworkOut),
            5 => Some(crate::app::SortKey::Cpu),
            _ => None,
        },
        Tab::Movers => match index {
            0 => Some(crate::app::SortKey::Pid),
            1 => Some(crate::app::SortKey::Name),
            2 => Some(crate::app::SortKey::User),
            3 => Some(crate::app::SortKey::Trend),
            _ => None,
        },
    }
}

fn colored_value_cell(text: &str, value: f64, width: usize) -> Cell<'static> {
    let color = if value >= 80.0 {
        RED
    } else if value >= 50.0 {
        YELLOW
    } else {
        TEXT
    };
    Cell::from(Span::styled(
        right(text.to_string(), width),
        Style::default().fg(color),
    ))
}

fn trend_arrow(delta: f64) -> String {
    if delta > 1.0 {
        " ↑".to_string()
    } else if delta < -1.0 {
        " ↓".to_string()
    } else {
        String::new()
    }
}

fn render_details(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
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
        let trend_width = (area.width as usize).saturating_sub(12).clamp(8, 40);
        if let Some(spark) = app
            .history()
            .process_cpu_sparkline(process.pid, trend_width)
        {
            lines.push(Line::from(vec![
                Span::styled(format!("{:<10}", "CPU trend"), Style::default().fg(MUTED)),
                Span::styled(spark, Style::default().fg(GREEN)),
            ]));
        }
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
            push_pair(
                &mut lines,
                "Priority",
                &details
                    .priority
                    .map(|priority| priority.to_string())
                    .unwrap_or_else(|| "-".to_string()),
            );
            if details.thread_count.is_none()
                || details.open_files.is_none()
                || details.session_id.is_none()
                || details.priority.is_none()
            {
                lines.push(Line::from(Span::styled(
                    "Some process details are hidden by macOS permissions.",
                    Style::default().fg(MUTED),
                )));
            }
        } else {
            push_pair(&mut lines, "Threads", "-");
            push_pair(&mut lines, "Open files", "-");
            push_pair(&mut lines, "Session", "-");
            push_pair(&mut lines, "Priority", "-");
            lines.push(Line::from(Span::styled(
                "Process details are unavailable or the process exited.",
                Style::default().fg(MUTED),
            )));
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

    let inner_height = area.height.saturating_sub(2) as usize;
    let max_scroll = lines.len().saturating_sub(inner_height);
    if app.inspector_scroll > max_scroll {
        app.inspector_scroll = max_scroll;
    }

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .title(inspector_title(app))
                .borders(Borders::ALL)
                .style(panel_style()),
        )
        .style(Style::default().fg(TEXT))
        .wrap(Wrap { trim: false })
        .scroll((app.inspector_scroll as u16, 0));
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
    let line = if app.filter_mode {
        Line::from(vec![
            Span::styled(
                " filter: ",
                Style::default().fg(YELLOW).add_modifier(Modifier::BOLD),
            ),
            Span::styled("type to search | ", Style::default().fg(TEXT)),
            Span::styled(
                "Backspace",
                Style::default().fg(GREEN).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" edits | ", Style::default().fg(TEXT)),
            Span::styled(
                "Enter/Esc",
                Style::default().fg(GREEN).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" keep focus | ", Style::default().fg(TEXT)),
            Span::styled(
                "Ctrl-U",
                Style::default().fg(GREEN).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" clears", Style::default().fg(TEXT)),
        ])
    } else {
        let sep = Span::styled(" │ ", Style::default().fg(MUTED));
        Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "1-6",
                Style::default().fg(GREEN).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" tabs ", Style::default().fg(MUTED)),
            Span::styled(
                "j/k",
                Style::default().fg(GREEN).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" move", Style::default().fg(MUTED)),
            sep.clone(),
            Span::styled("/", Style::default().fg(GREEN).add_modifier(Modifier::BOLD)),
            Span::styled(" filter ", Style::default().fg(MUTED)),
            Span::styled(
                "s/S",
                Style::default().fg(GREEN).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" sort", Style::default().fg(MUTED)),
            sep.clone(),
            Span::styled("i", Style::default().fg(GREEN).add_modifier(Modifier::BOLD)),
            Span::styled(" inspect ", Style::default().fg(MUTED)),
            Span::styled(
                "Enter",
                Style::default().fg(GREEN).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" files ", Style::default().fg(MUTED)),
            Span::styled("o", Style::default().fg(GREEN).add_modifier(Modifier::BOLD)),
            Span::styled(" overview", Style::default().fg(MUTED)),
            sep.clone(),
            Span::styled(
                "t/f",
                Style::default().fg(GREEN).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" term/kill ", Style::default().fg(MUTED)),
            Span::styled(
                "z/g",
                Style::default().fg(GREEN).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" stop/cont ", Style::default().fg(MUTED)),
            Span::styled(
                "[]",
                Style::default().fg(GREEN).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" nice", Style::default().fg(MUTED)),
            sep.clone(),
            Span::styled(
                "+/-",
                Style::default().fg(GREEN).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" refresh ", Style::default().fg(MUTED)),
            Span::styled("?", Style::default().fg(GREEN).add_modifier(Modifier::BOLD)),
            Span::styled(" help ", Style::default().fg(MUTED)),
            Span::styled("q", Style::default().fg(GREEN).add_modifier(Modifier::BOLD)),
            Span::styled(" quit", Style::default().fg(MUTED)),
        ])
    };

    let footer = Paragraph::new(line).style(Style::default().bg(PANEL));
    frame.render_widget(footer, area);
}

fn render_help(frame: &mut Frame<'_>, area: Rect, app: &mut App) {
    let popup = centered_rect(90, 88, area);
    frame.render_widget(Clear, popup);
    let mut lines = vec![
        Line::from(Span::styled(
            "monitr controls",
            Style::default().fg(GREEN).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    macro_rules! add_help_line {
        ($key:expr, $desc:expr) => {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{:<16}", $key),
                    Style::default().fg(YELLOW).add_modifier(Modifier::BOLD),
                ),
                Span::styled($desc.to_string(), Style::default().fg(TEXT)),
            ]));
        };
    }

    add_help_line!("1-6 / Tab", "switch category");
    add_help_line!("j/k or arrows", "move process selection");
    add_help_line!("Page/Home/End", "jump through the process table");
    add_help_line!("click", "select a process row with the mouse");
    add_help_line!("/", "filter by name, pid, user, command, or status");
    lines.push(Line::from(vec![
        Span::styled(format!("{:<16}", ""), Style::default()),
        Span::styled("predicates: ", Style::default().fg(MUTED)),
        Span::styled(
            "cpu>50  mem<100mb  user:milo  name:node",
            Style::default().fg(BLUE),
        ),
    ]));
    add_help_line!("Ctrl-U", "clear the active filter anywhere");
    add_help_line!("s / S", "cycle sort key / reverse sort");
    add_help_line!(
        "c m e d D n p T u",
        "sort CPU, memory, impact, write, read, name, pid, runtime, user"
    );
    add_help_line!("i", "show or hide process inspector");
    add_help_line!("o", "show or hide the overview panel");
    add_help_line!("x", "toggle compact table mode");
    add_help_line!("R", "reset persisted preferences to defaults");
    add_help_line!("Ctrl-J / Ctrl-K", "scroll the inspector panel");
    add_help_line!("Enter", "open files and sockets for the selected process");
    add_help_line!("t / f", "send TERM / KILL after confirmation");
    add_help_line!(
        "z / g",
        "suspend / resume with STOP / CONT after confirmation"
    );
    add_help_line!(
        "[ / ]",
        "lower / raise process priority by 5 after confirmation"
    );
    add_help_line!("+ / -", "slower / faster refresh interval");
    add_help_line!("r", "refresh immediately");
    add_help_line!("q, Esc, Ctrl-C", "quit");
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Disk and Network tables show per-process rates; inspector panels show system-level volumes/interfaces.",
        Style::default().fg(TEXT),
    )));
    lines.push(Line::from(Span::styled(
        "Movers shows CPU, memory, disk, and network changes since the previous sample.",
        Style::default().fg(TEXT),
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Press j/k to scroll. Press Esc, Enter, ?, or q to close.",
        Style::default().fg(MUTED),
    )));

    let inner_height = popup.height.saturating_sub(2) as usize;
    let max_scroll = lines.len().saturating_sub(inner_height);
    if app.help_scroll > max_scroll {
        app.help_scroll = max_scroll;
    }

    let help = Paragraph::new(lines)
        .block(
            Block::default()
                .title("Help")
                .borders(Borders::ALL)
                .style(panel_style()),
        )
        .style(Style::default().fg(TEXT))
        .wrap(Wrap { trim: true })
        .scroll((app.help_scroll as u16, 0));
    frame.render_widget(help, popup);
}

fn render_handles(frame: &mut Frame<'_>, area: Rect, view: &HandlesView) {
    let popup = centered_rect(86, 86, area);
    frame.render_widget(Clear, popup);

    let inner_height = popup.height.saturating_sub(2) as usize;
    let overhead = 8 + usize::from(view.error.is_some());
    let per_section = inner_height.saturating_sub(overhead) / 2;
    let per_section = per_section.max(3);
    let name_width = (popup.width as usize).saturating_sub(18).clamp(12, 120);

    let mut lines = vec![Line::from(vec![
        Span::styled(
            format!("{} ", view.name),
            Style::default().fg(GREEN).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("pid {}", view.pid), Style::default().fg(MUTED)),
    ])];

    if view.loading {
        lines.push(Line::from(""));
        lines.push(muted_line("Loading handles..."));
        lines.push(Line::from(""));
        lines.push(muted_line("Press Esc, Enter, o, or q to close."));
    } else {
        if let Some(error) = &view.error {
            lines.push(Line::from(Span::styled(
                error.clone(),
                Style::default().fg(RED),
            )));
        }

        lines.push(Line::from(""));
        lines.push(section_header(format!("Sockets ({})", view.sockets.len())));
        if view.sockets.is_empty() {
            lines.push(muted_line("none visible"));
        } else {
            for socket in view.sockets.iter().take(per_section) {
                let remote_or_state = socket
                    .remote
                    .as_deref()
                    .or(socket.state.as_deref())
                    .unwrap_or("-");
                let proto_color = if socket.protocol.to_uppercase() == "TCP" {
                    GREEN
                } else {
                    BLUE
                };
                let state_color = if remote_or_state == "LISTEN" {
                    YELLOW
                } else if remote_or_state == "ESTABLISHED" {
                    GREEN
                } else {
                    TEXT
                };
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("{:<6}", format::truncate_middle(&socket.fd, 5)),
                        Style::default().fg(MUTED),
                    ),
                    Span::styled(
                        format!("{:<6}", socket.protocol),
                        Style::default()
                            .fg(proto_color)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("{:<29}", format::truncate_middle(&socket.local, 28)),
                        Style::default().fg(TEXT),
                    ),
                    Span::styled(
                        remote_or_state.to_string(),
                        Style::default().fg(state_color),
                    ),
                ]));
            }
            if view.sockets.len() > per_section {
                lines.push(muted_line(&overflow_hint(
                    view.sockets.len() - per_section,
                    view.pid,
                )));
            }
        }

        lines.push(Line::from(""));
        lines.push(section_header(format!("Open files ({})", view.files.len())));
        if view.files.is_empty() {
            lines.push(muted_line("none visible"));
        } else {
            for file in view.files.iter().take(per_section) {
                let type_color = match file.file_type.as_str() {
                    "DIR" => BLUE,
                    "REG" => TEXT,
                    "PIPE" => YELLOW,
                    _ => MUTED,
                };
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("{:<6}", format::truncate_middle(&file.fd, 5)),
                        Style::default().fg(MUTED),
                    ),
                    Span::styled(
                        format!("{:<6}", file.file_type),
                        Style::default().fg(type_color),
                    ),
                    Span::styled(
                        format::truncate_middle(&file.name, name_width),
                        Style::default().fg(TEXT),
                    ),
                ]));
            }
            if view.files.len() > per_section {
                lines.push(muted_line(&overflow_hint(
                    view.files.len() - per_section,
                    view.pid,
                )));
            }
        }

        lines.push(Line::from(""));
        lines.push(muted_line("Press Esc, Enter, o, or q to close."));
    }

    let panel = Paragraph::new(lines)
        .block(
            Block::default()
                .title("Open files & sockets")
                .borders(Borders::ALL)
                .style(panel_style()),
        )
        .style(Style::default().fg(TEXT));
    frame.render_widget(panel, popup);
}

fn section_header(text: String) -> Line<'static> {
    Line::from(Span::styled(
        text,
        Style::default().fg(GREEN).add_modifier(Modifier::BOLD),
    ))
}

fn muted_line(text: &str) -> Line<'static> {
    Line::from(Span::styled(text.to_string(), Style::default().fg(MUTED)))
}

fn overflow_hint(extra: usize, pid: u32) -> String {
    format!("+{extra} more — run monitr inspect {pid}")
}

fn push_pair(lines: &mut Vec<Line<'static>>, label: &str, value: &str) {
    lines.push(Line::from(vec![
        Span::styled(format!("{label:<10}"), Style::default().fg(MUTED)),
        Span::styled(value.to_string(), Style::default().fg(TEXT)),
    ]));
}

fn right(value: String, width: usize) -> String {
    let len = value.len();
    if len >= width {
        value
    } else {
        format!("{}{}", " ".repeat(width - len), value)
    }
}

fn process_style(process: &ProcessRow) -> Style {
    let fg = if process.status == "zombie" || process.status == "dead" {
        RED
    } else if process.trend.new_process {
        BLUE
    } else if process.cpu_usage >= 75.0 {
        YELLOW
    } else {
        TEXT
    };
    Style::default().fg(fg).bg(PANEL)
}

fn panel_style() -> Style {
    Style::default().fg(MUTED).bg(PANEL)
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

fn render_usage_chart(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &'static str,
    values: &[f64],
    scale_max: f64,
    current_percent: f64,
    summary_lines: &[String],
) {
    let block = metric_block(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width < 14 || inner.height < 3 {
        return;
    }

    let lines = build_usage_chart_lines(
        inner.width as usize,
        inner.height as usize,
        values,
        scale_max,
        current_percent,
        summary_lines,
    );
    frame.render_widget(Paragraph::new(lines), inner);
}

fn build_usage_chart_lines(
    width: usize,
    height: usize,
    values: &[f64],
    scale_max: f64,
    current_percent: f64,
    summary_lines: &[String],
) -> Vec<Line<'static>> {
    let summary_width = (width / 3).clamp(10, 14).min(width.saturating_sub(6));
    let chart_width = width.saturating_sub(summary_width + 1);
    if chart_width < 6 || summary_width == 0 {
        return vec![Line::from(center_text(width, &summary_lines.join(" | ")))];
    }

    let aligned_values = align_chart_values(values, chart_width);
    let summary_bg = summary_bg_color(current_percent);
    let summary_fg = summary_fg_color(current_percent);
    let summary_fill = filled_rows(current_percent, 100.0, height);
    let summary_start = height.saturating_sub(summary_lines.len()) / 2;

    (0..height)
        .map(|row| {
            let mut spans = Vec::with_capacity(chart_width + 2);

            for value in &aligned_values {
                let (ch, style) = match value {
                    Some(v) => {
                        let filled_float = (v / scale_max).clamp(0.0, 1.0) * height as f64;
                        let full_rows = filled_float.floor() as usize;
                        let frac = filled_float.fract();
                        let partial_row = height.saturating_sub(full_rows + 1);

                        if row >= height.saturating_sub(full_rows) {
                            let fill = usage_band_color(row, height);
                            (' ', Style::default().fg(fill).bg(fill))
                        } else if row == partial_row && frac > 0.0 {
                            let fill = usage_band_color(row, height);
                            let ch = partial_block(frac);
                            (ch, Style::default().fg(fill).bg(PANEL_ALT))
                        } else {
                            (' ', Style::default().fg(MUTED).bg(PANEL_ALT))
                        }
                    }
                    None => (' ', Style::default().fg(MUTED).bg(PANEL_ALT)),
                };
                spans.push(Span::styled(ch.to_string(), style));
            }

            let summary_filled = row >= height.saturating_sub(summary_fill);
            let summary_row_bg = if summary_filled {
                summary_bg
            } else {
                PANEL_ALT
            };
            let summary_row_fg = if summary_filled { summary_fg } else { TEXT };

            spans.push(Span::styled(
                "│",
                Style::default().fg(MUTED).bg(summary_row_bg),
            ));

            let summary_index = row.checked_sub(summary_start);
            let is_headline = summary_index == Some(0);
            let text = summary_index
                .and_then(|index| summary_lines.get(index))
                .map_or_else(
                    || " ".repeat(summary_width),
                    |line| {
                        center_text(summary_width, &format::truncate_middle(line, summary_width))
                    },
                );
            let summary_style = if is_headline {
                Style::default()
                    .fg(summary_row_fg)
                    .bg(summary_row_bg)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(summary_row_fg).bg(summary_row_bg)
            };
            spans.push(Span::styled(text, summary_style));

            Line::from(spans)
        })
        .collect()
}

fn align_chart_values(values: &[f64], width: usize) -> Vec<Option<f64>> {
    let mut aligned = vec![None; width];
    if width == 0 || values.is_empty() {
        return aligned;
    }

    for (slot, value) in aligned
        .iter_mut()
        .rev()
        .zip(values.iter().rev().take(width))
    {
        *slot = Some(*value);
    }
    aligned
}

fn filled_rows(value: f64, scale_max: f64, height: usize) -> usize {
    if scale_max <= 0.0 {
        return 0;
    }
    ((value / scale_max).clamp(0.0, 1.0) * height as f64).ceil() as usize
}

fn partial_block(frac: f64) -> char {
    match (frac * 8.0).ceil() as u8 {
        0 => ' ',
        1 => '▁',
        2 => '▂',
        3 => '▃',
        4 => '▄',
        5 => '▅',
        6 => '▆',
        7 => '▇',
        _ => '█',
    }
}

fn usage_band_color(row: usize, height: usize) -> Color {
    let percent = ((height.saturating_sub(row)) as f64 / height.max(1) as f64) * 100.0;
    usage_color(percent)
}

fn summary_bg_color(value: f64) -> Color {
    if value >= 85.0 {
        SUMMARY_RED
    } else if value >= 60.0 {
        SUMMARY_YELLOW
    } else {
        SUMMARY_GREEN
    }
}

fn summary_fg_color(value: f64) -> Color {
    if value >= 60.0 { BG } else { TEXT }
}

fn chart_width(area: Rect) -> usize {
    area.width.saturating_sub(14) as usize
}

fn center_text(width: usize, text: &str) -> String {
    let text_width = text.chars().count().min(width);
    let left = width.saturating_sub(text_width) / 2;
    let right = width.saturating_sub(text_width + left);
    format!("{}{}{}", " ".repeat(left), text, " ".repeat(right))
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

#[cfg(test)]
mod tests {
    use super::{
        ProcessTableView, Tab, build_usage_chart_lines, column_sort_key, column_widths,
        process_table_view,
    };
    use crate::app::SortKey;

    #[test]
    fn compact_table_widths_fit_narrow_panes() {
        for tab in Tab::ALL {
            let widths = column_widths(tab, crate::app::SortKey::Pid, true, true, 60);
            let spacing = widths.len().saturating_sub(1);
            assert!(
                widths.iter().sum::<usize>() + spacing <= 58,
                "{tab:?} compact widths overflowed: {widths:?}"
            );
        }
    }

    #[test]
    fn compact_table_columns_keep_primary_sort_targets() {
        assert_eq!(column_sort_key(Tab::Cpu, false, 2, 60), Some(SortKey::Cpu));
        assert_eq!(
            column_sort_key(Tab::Memory, false, 2, 60),
            Some(SortKey::Memory)
        );
        assert_eq!(
            column_sort_key(Tab::Energy, false, 2, 60),
            Some(SortKey::Energy)
        );
        assert_eq!(
            column_sort_key(Tab::Disk, false, 2, 60),
            Some(SortKey::DiskRead)
        );
        assert_eq!(
            column_sort_key(Tab::Disk, false, 3, 60),
            Some(SortKey::DiskWrite)
        );
        assert_eq!(
            column_sort_key(Tab::Network, false, 2, 60),
            Some(SortKey::NetworkIn)
        );
        assert_eq!(
            column_sort_key(Tab::Network, false, 3, 60),
            Some(SortKey::NetworkOut)
        );
        assert_eq!(
            column_sort_key(Tab::Movers, false, 2, 60),
            Some(SortKey::Trend)
        );
    }

    #[test]
    fn usage_charts_do_not_render_horizontal_guides() {
        let lines = build_usage_chart_lines(
            40,
            6,
            &[10.0, 25.0, 50.0, 75.0],
            100.0,
            75.0,
            &[
                "75%".to_string(),
                "across".to_string(),
                "12 Cores".to_string(),
            ],
        );
        let rendered = lines
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");

        assert!(!rendered.contains('╌'));
        assert!(rendered.contains("75%"));
    }

    #[test]
    fn process_table_view_limits_rendered_rows_to_the_viewport() {
        let view = process_table_view(1_000, Some(500), 480, 12);

        assert_eq!(
            view,
            ProcessTableView {
                start: 489,
                end: 501,
                selected: Some(11),
            }
        );
    }

    #[test]
    fn process_table_view_clamps_offset_without_selection() {
        let view = process_table_view(5, None, 99, 3);

        assert_eq!(
            view,
            ProcessTableView {
                start: 2,
                end: 5,
                selected: None,
            }
        );
    }

    #[test]
    fn process_table_view_returns_empty_when_no_rows_fit() {
        let view = process_table_view(10, Some(4), 0, 0);

        assert_eq!(
            view,
            ProcessTableView {
                start: 0,
                end: 0,
                selected: None,
            }
        );
    }
}
