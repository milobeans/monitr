use std::fmt::Write as _;

use serde::Serialize;

use crate::{
    error::Result,
    filter::Filter,
    format,
    sampler::{DiskRow, NetworkRow, ProcessRow, Snapshot, SystemTotals},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapshotOptions<'a> {
    pub filter: Option<&'a str>,
    pub limit: Option<usize>,
    pub json: bool,
}

pub fn render_snapshot(snapshot: &Snapshot, options: SnapshotOptions<'_>) -> Result<String> {
    let processes = filtered_processes(snapshot, options.filter, options.limit);
    if options.json {
        return Ok(serde_json::to_string_pretty(&SnapshotDocument::new(
            snapshot,
            options.filter,
            processes,
        ))?);
    }

    Ok(render_snapshot_text(snapshot, options.filter, processes))
}

fn filtered_processes<'a>(
    snapshot: &'a Snapshot,
    filter: Option<&str>,
    limit: Option<usize>,
) -> Vec<&'a ProcessRow> {
    let filter = filter.map(|value| Filter::parse(value.trim()));
    let mut processes = snapshot
        .processes
        .iter()
        .filter(|process| filter.as_ref().is_none_or(|filter| filter.matches(process)))
        .collect::<Vec<_>>();
    processes.sort_by(|left, right| {
        right
            .cpu_usage
            .total_cmp(&left.cpu_usage)
            .then_with(|| right.memory.cmp(&left.memory))
            .then_with(|| left.sort_name.cmp(&right.sort_name))
    });
    if let Some(limit) = limit {
        processes.truncate(limit);
    }
    processes
}

fn render_snapshot_text(
    snapshot: &Snapshot,
    filter: Option<&str>,
    processes: Vec<&ProcessRow>,
) -> String {
    let mut out = String::new();
    let totals = &snapshot.totals;
    let filter = filter
        .filter(|value| !value.trim().is_empty())
        .map(|value| format!(" | filter {value}"))
        .unwrap_or_default();

    let _ = writeln!(
        out,
        "CPU {} | Memory {} / {} | Disk R {} W {} | Network In {} Out {} | sample {:.2}s{}",
        format::percent(totals.cpu_usage as f64),
        format::bytes(totals.used_memory),
        format::bytes(totals.total_memory),
        format::bytes_rate(totals.disk_read_rate),
        format::bytes_rate(totals.disk_write_rate),
        format::bytes_rate(totals.net_in_rate),
        format::bytes_rate(totals.net_out_rate),
        snapshot.sample_span.as_secs_f64(),
        filter,
    );
    let _ = writeln!(
        out,
        "{:>7} {:>7} {:>10} {:>12} {:<13} NAME",
        "PID", "%CPU", "MEMORY", "DISK/S", "USER"
    );
    for process in processes {
        let _ = writeln!(
            out,
            "{:>7} {:>7} {:>10} {:>12} {:<13} {}",
            process.pid,
            format::percent(process.cpu_usage as f64),
            format::bytes(process.memory),
            format::bytes_rate(process.disk_read_rate + process.disk_write_rate),
            format::truncate_middle(&process.user, 13),
            process.name,
        );
    }
    out
}

#[derive(Serialize)]
struct SnapshotDocument<'a> {
    sample_span_ms: u128,
    process_count: usize,
    shown_process_count: usize,
    filter: Option<&'a str>,
    totals: TotalsDocument<'a>,
    processes: Vec<ProcessDocument<'a>>,
    disks: Vec<DiskDocument<'a>>,
    networks: Vec<NetworkDocument<'a>>,
}

impl<'a> SnapshotDocument<'a> {
    fn new(
        snapshot: &'a Snapshot,
        filter: Option<&'a str>,
        processes: Vec<&'a ProcessRow>,
    ) -> Self {
        Self {
            sample_span_ms: snapshot.sample_span.as_millis(),
            process_count: snapshot.process_count,
            shown_process_count: processes.len(),
            filter,
            totals: TotalsDocument::new(&snapshot.totals),
            processes: processes.into_iter().map(ProcessDocument::new).collect(),
            disks: snapshot.disks.iter().map(DiskDocument::new).collect(),
            networks: snapshot.networks.iter().map(NetworkDocument::new).collect(),
        }
    }
}

#[derive(Serialize)]
struct TotalsDocument<'a> {
    host: &'a str,
    os: &'a str,
    uptime_seconds: u64,
    cpu_usage_percent: f32,
    cpu_count: usize,
    total_memory_bytes: u64,
    used_memory_bytes: u64,
    total_swap_bytes: u64,
    used_swap_bytes: u64,
    disk_read_bytes_per_sec: f64,
    disk_write_bytes_per_sec: f64,
    network_in_bytes_per_sec: f64,
    network_out_bytes_per_sec: f64,
}

impl<'a> TotalsDocument<'a> {
    fn new(totals: &'a SystemTotals) -> Self {
        Self {
            host: &totals.host,
            os: &totals.os,
            uptime_seconds: totals.uptime,
            cpu_usage_percent: totals.cpu_usage,
            cpu_count: totals.cpu_count,
            total_memory_bytes: totals.total_memory,
            used_memory_bytes: totals.used_memory,
            total_swap_bytes: totals.total_swap,
            used_swap_bytes: totals.used_swap,
            disk_read_bytes_per_sec: totals.disk_read_rate,
            disk_write_bytes_per_sec: totals.disk_write_rate,
            network_in_bytes_per_sec: totals.net_in_rate,
            network_out_bytes_per_sec: totals.net_out_rate,
        }
    }
}

#[derive(Serialize)]
struct ProcessDocument<'a> {
    pid: u32,
    parent_pid: Option<u32>,
    name: &'a str,
    user: &'a str,
    command: &'a str,
    executable: &'a str,
    cwd: &'a str,
    status: &'a str,
    cpu_usage_percent: f32,
    memory_bytes: u64,
    virtual_memory_bytes: u64,
    memory_percent: f64,
    disk_read_bytes_per_sec: f64,
    disk_write_bytes_per_sec: f64,
    total_disk_read_bytes: u64,
    total_disk_written_bytes: u64,
    runtime_seconds: u64,
    start_time_unix: u64,
    energy_impact: f64,
    cpu_delta_percent: f32,
    memory_delta_bytes: i64,
    disk_read_delta_bytes_per_sec: f64,
    disk_write_delta_bytes_per_sec: f64,
    change_score: f64,
    new_process: bool,
    thread_count: Option<usize>,
    open_files: Option<usize>,
    open_files_limit: Option<usize>,
    session_id: Option<u32>,
    priority: Option<i32>,
}

impl<'a> ProcessDocument<'a> {
    fn new(process: &'a ProcessRow) -> Self {
        let details = process.selected_details.as_ref();
        Self {
            pid: process.pid,
            parent_pid: process.parent_pid,
            name: &process.name,
            user: &process.user,
            command: &process.command,
            executable: &process.exe,
            cwd: &process.cwd,
            status: &process.status,
            cpu_usage_percent: process.cpu_usage,
            memory_bytes: process.memory,
            virtual_memory_bytes: process.virtual_memory,
            memory_percent: process.memory_percent,
            disk_read_bytes_per_sec: process.disk_read_rate,
            disk_write_bytes_per_sec: process.disk_write_rate,
            total_disk_read_bytes: process.total_disk_read,
            total_disk_written_bytes: process.total_disk_write,
            runtime_seconds: process.run_time,
            start_time_unix: process.start_time,
            energy_impact: process.energy_impact,
            cpu_delta_percent: process.trend.cpu_delta,
            memory_delta_bytes: process.trend.memory_delta,
            disk_read_delta_bytes_per_sec: process.trend.disk_read_rate_delta,
            disk_write_delta_bytes_per_sec: process.trend.disk_write_rate_delta,
            change_score: process.trend.score(),
            new_process: process.trend.new_process,
            thread_count: details.and_then(|details| details.thread_count),
            open_files: details.and_then(|details| details.open_files),
            open_files_limit: details.and_then(|details| details.open_files_limit),
            session_id: details.and_then(|details| details.session_id),
            priority: details.and_then(|details| details.priority),
        }
    }
}

#[derive(Serialize)]
struct DiskDocument<'a> {
    name: &'a str,
    mount_point: &'a str,
    total_bytes: u64,
    available_bytes: u64,
    read_bytes_per_sec: f64,
    write_bytes_per_sec: f64,
}

impl<'a> DiskDocument<'a> {
    fn new(disk: &'a DiskRow) -> Self {
        Self {
            name: &disk.name,
            mount_point: &disk.mount_point,
            total_bytes: disk.total,
            available_bytes: disk.available,
            read_bytes_per_sec: disk.read_rate,
            write_bytes_per_sec: disk.write_rate,
        }
    }
}

#[derive(Serialize)]
struct NetworkDocument<'a> {
    name: &'a str,
    received_bytes_per_sec: f64,
    transmitted_bytes_per_sec: f64,
    total_received_bytes: u64,
    total_transmitted_bytes: u64,
}

impl<'a> NetworkDocument<'a> {
    fn new(network: &'a NetworkRow) -> Self {
        Self {
            name: &network.name,
            received_bytes_per_sec: network.received_rate,
            transmitted_bytes_per_sec: network.transmitted_rate,
            total_received_bytes: network.total_received,
            total_transmitted_bytes: network.total_transmitted,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::sampler::{Snapshot, SystemTotals};

    use super::{SnapshotOptions, render_snapshot};

    #[test]
    fn renders_snapshot_json_with_machine_readable_fields() {
        let snapshot = Snapshot {
            totals: SystemTotals {
                cpu_usage: 12.5,
                cpu_count: 8,
                total_memory: 100,
                used_memory: 50,
                total_swap: 20,
                used_swap: 2,
                disk_read_rate: 1.0,
                disk_write_rate: 2.0,
                net_in_rate: 3.0,
                net_out_rate: 4.0,
                uptime: 99,
                host: "host".into(),
                os: "macOS".into(),
            },
            processes: Vec::new(),
            disks: Vec::new(),
            networks: Vec::new(),
            sample_span: Duration::from_millis(250),
            process_count: 0,
        };

        let rendered = render_snapshot(
            &snapshot,
            SnapshotOptions {
                filter: None,
                limit: None,
                json: true,
            },
        )
        .unwrap();

        assert!(rendered.contains("\"cpu_usage_percent\": 12.5"));
        assert!(rendered.contains("\"sample_span_ms\": 250"));
    }
}
