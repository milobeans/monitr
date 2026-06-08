use std::{
    fmt::Write as _,
    process::{Command, Output},
};

use serde::Serialize;

use crate::{
    error::{self, Result},
    format,
    sampler::{ProcessRow, Sampler},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InspectOptions {
    pub pid: u32,
    pub json: bool,
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Inspection {
    pub process: InspectProcess,
    pub files: Vec<FileEntry>,
    pub sockets: Vec<SocketEntry>,
}

/// The open files and sockets held by a process, independent of the heavier
/// `Inspection` (which also re-samples process metadata). The TUI uses this to
/// surface handles for the already-selected process without a fresh sample.
#[derive(Debug, Clone, Default)]
pub struct ProcessHandles {
    pub files: Vec<FileEntry>,
    pub sockets: Vec<SocketEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InspectProcess {
    pub pid: u32,
    pub parent_pid: Option<u32>,
    pub name: String,
    pub user: String,
    pub command: String,
    pub executable: String,
    pub cwd: String,
    pub status: String,
    pub cpu_usage_percent: f32,
    pub memory_bytes: u64,
    pub virtual_memory_bytes: u64,
    pub disk_read_bytes_per_sec: f64,
    pub disk_write_bytes_per_sec: f64,
    pub runtime_seconds: u64,
    pub energy_impact: f64,
    pub thread_count: Option<usize>,
    pub open_files: Option<usize>,
    pub open_files_limit: Option<usize>,
    pub session_id: Option<u32>,
    pub priority: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileEntry {
    pub fd: String,
    pub file_type: String,
    pub device: Option<String>,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SocketEntry {
    pub fd: String,
    pub protocol: String,
    pub local: String,
    pub remote: Option<String>,
    pub state: Option<String>,
}

pub fn inspect(options: InspectOptions) -> Result<Inspection> {
    let mut sampler = Sampler::new()?;
    let snapshot = sampler.sample(Some(options.pid));
    let process = snapshot
        .processes
        .iter()
        .find(|process| process.pid == options.pid)
        .ok_or_else(|| error::message(format!("process {} is not visible", options.pid)))?;

    let handles = collect_handles(options.pid)?;
    Ok(Inspection {
        process: InspectProcess::from(process),
        files: handles.files,
        sockets: handles.sockets,
    })
}

pub fn render(inspection: &Inspection, options: InspectOptions) -> Result<String> {
    if options.json {
        return Ok(serde_json::to_string_pretty(inspection)?);
    }

    let mut out = String::new();
    let process = &inspection.process;
    let _ = writeln!(
        out,
        "{} pid {} | user {} | status {} | priority {}",
        process.name,
        process.pid,
        process.user,
        process.status,
        process
            .priority
            .map(|priority| priority.to_string())
            .unwrap_or_else(|| "-".to_string())
    );
    let _ = writeln!(
        out,
        "CPU {} | Memory {} | Disk R {} W {} | runtime {} | impact {}",
        format::percent(process.cpu_usage_percent as f64),
        format::bytes(process.memory_bytes),
        format::bytes_rate(process.disk_read_bytes_per_sec),
        format::bytes_rate(process.disk_write_bytes_per_sec),
        format::duration(process.runtime_seconds),
        format::number(process.energy_impact),
    );
    let _ = writeln!(out, "cwd: {}", process.cwd);
    let _ = writeln!(out, "exe: {}", process.executable);
    let _ = writeln!(out, "cmd: {}", process.command);

    let _ = writeln!(out);
    let _ = writeln!(out, "Sockets");
    if inspection.sockets.is_empty() {
        let _ = writeln!(out, "  none visible");
    } else {
        for socket in inspection.sockets.iter().take(options.limit) {
            let remote_or_state = socket
                .remote
                .as_deref()
                .or(socket.state.as_deref())
                .unwrap_or("-");
            let _ = writeln!(
                out,
                "  {:<5} {:<5} {:<32} {}",
                socket.fd,
                socket.protocol,
                format::truncate_middle(&socket.local, 32),
                remote_or_state,
            );
        }
    }

    let _ = writeln!(out);
    let _ = writeln!(out, "Open files");
    if inspection.files.is_empty() {
        let _ = writeln!(out, "  none visible");
    } else {
        for file in inspection.files.iter().take(options.limit) {
            let _ = writeln!(
                out,
                "  {:<5} {:<5} {}",
                file.fd,
                file.file_type,
                format::truncate_middle(&file.name, 88),
            );
        }
    }
    Ok(out)
}

impl From<&ProcessRow> for InspectProcess {
    fn from(process: &ProcessRow) -> Self {
        let details = process.selected_details.as_ref();
        Self {
            pid: process.pid,
            parent_pid: process.parent_pid,
            name: process.name.clone(),
            user: process.user.clone(),
            command: process.command.clone(),
            executable: process.exe.clone(),
            cwd: process.cwd.clone(),
            status: process.status.clone(),
            cpu_usage_percent: process.cpu_usage,
            memory_bytes: process.memory,
            virtual_memory_bytes: process.virtual_memory,
            disk_read_bytes_per_sec: process.disk_read_rate,
            disk_write_bytes_per_sec: process.disk_write_rate,
            runtime_seconds: process.run_time,
            energy_impact: process.energy_impact,
            thread_count: details.and_then(|details| details.thread_count),
            open_files: details.and_then(|details| details.open_files),
            open_files_limit: details.and_then(|details| details.open_files_limit),
            session_id: details.and_then(|details| details.session_id),
            priority: details.and_then(|details| details.priority),
        }
    }
}

pub fn collect_handles(pid: u32) -> Result<ProcessHandles> {
    let output = Command::new("lsof")
        .args(["-nP", "-p", &pid.to_string(), "-F", "ftDnPnT"])
        .output()?;
    if !output.status.success() && output.stdout.is_empty() {
        if is_empty_lsof_result(&output) {
            return Ok(ProcessHandles::default());
        }
        return Err(error::message(lsof_failure_message("handles", &output)));
    }
    let (files, sockets) = parse_lsof_combined(&String::from_utf8_lossy(&output.stdout));
    Ok(ProcessHandles { files, sockets })
}

fn is_empty_lsof_result(output: &Output) -> bool {
    output.status.code() == Some(1)
}

fn lsof_failure_message(kind: &str, output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stderr = stderr.trim();
    if stderr.is_empty() {
        format!(
            "lsof {kind} failed with status {}",
            output
                .status
                .code()
                .map(|code| code.to_string())
                .unwrap_or_else(|| "signal".to_string())
        )
    } else {
        format!("lsof {kind} failed: {stderr}")
    }
}

#[derive(Default)]
struct DescriptorFields {
    fd: String,
    file_type: String,
    device: Option<String>,
    name: String,
    protocol: String,
    local: String,
    remote: Option<String>,
    state: Option<String>,
}

fn parse_lsof_combined(output: &str) -> (Vec<FileEntry>, Vec<SocketEntry>) {
    let mut files = Vec::new();
    let mut sockets = Vec::new();
    let mut current = DescriptorFields::default();

    for line in output.lines().filter(|line| !line.is_empty()) {
        let mut chars = line.chars();
        let Some(field) = chars.next() else {
            continue;
        };
        let value = chars.as_str();

        match field {
            'f' => {
                flush_descriptor(&mut current, &mut files, &mut sockets);
                current.fd = value.to_string();
            }
            't' => current.file_type = value.to_string(),
            'D' => current.device = Some(value.to_string()),
            'n' => {
                current.name = value.to_string();
                let (local, remote) = value
                    .split_once("->")
                    .map(|(local, remote)| (local.to_string(), Some(remote.to_string())))
                    .unwrap_or_else(|| (value.to_string(), None));
                current.local = local;
                current.remote = remote;
            }
            'P' => current.protocol = value.to_string(),
            'T' => {
                if let Some(state) = value.strip_prefix("ST=") {
                    current.state = Some(state.to_string());
                }
            }
            _ => {}
        }
    }
    flush_descriptor(&mut current, &mut files, &mut sockets);
    (files, sockets)
}

fn flush_descriptor(
    current: &mut DescriptorFields,
    files: &mut Vec<FileEntry>,
    sockets: &mut Vec<SocketEntry>,
) {
    if current.fd.is_empty() {
        *current = DescriptorFields::default();
        return;
    }

    let is_socket = !current.protocol.is_empty() || current.file_type.starts_with("IPv");

    if is_socket {
        if !current.local.is_empty() {
            sockets.push(SocketEntry {
                fd: current.fd.clone(),
                protocol: value_or_dash(&current.protocol),
                local: current.local.clone(),
                remote: current.remote.clone(),
                state: current.state.clone(),
            });
        }
    } else if !current.name.is_empty() {
        files.push(FileEntry {
            fd: current.fd.clone(),
            file_type: value_or_dash(&current.file_type),
            device: current.device.clone(),
            name: current.name.clone(),
        });
    }
    *current = DescriptorFields::default();
}

fn value_or_dash(value: &str) -> String {
    if value.is_empty() {
        "-".to_string()
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::parse_lsof_combined;

    #[test]
    fn parses_lsof_combined_entries() {
        let (files, sockets) = parse_lsof_combined(
            "\
p34138
fcwd
tDIR
D0x1000011
n/Users/miloevans/monitr
f1
tPIPE
n->0x42f88a3c7b8a6378
f18
tIPv4
PTCP
n127.0.0.1:18789
TST=LISTEN
f24
tIPv6
PUDP
n[::1]:5353->[ff02::fb]:5353
",
        );

        assert_eq!(files.len(), 2);
        assert_eq!(files[0].fd, "cwd");
        assert_eq!(files[0].file_type, "DIR");
        assert_eq!(files[0].name, "/Users/miloevans/monitr");
        assert_eq!(files[1].fd, "1");
        assert_eq!(files[1].file_type, "PIPE");

        assert_eq!(sockets.len(), 2);
        assert_eq!(sockets[0].protocol, "TCP");
        assert_eq!(sockets[0].state.as_deref(), Some("LISTEN"));
        assert_eq!(sockets[1].remote.as_deref(), Some("[ff02::fb]:5353"));
    }
}
