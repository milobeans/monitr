use std::{
    fmt::Write as _,
    process::{Command, Output},
    sync::OnceLock,
    time::Duration,
};

use serde::Serialize;

use crate::{
    error::{self, Result},
    format,
    process_record::ProcessRecord,
    sampler::Sampler,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InspectOptions {
    pub pid: u32,
    pub json: bool,
    pub limit: usize,
    pub full: bool,
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

pub type InspectProcess = ProcessRecord;

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

pub fn inspect(options: InspectOptions, interval: Duration) -> Result<Inspection> {
    let mut sampler = Sampler::new()?;
    let baseline = sampler.sample(Some(options.pid));
    let previous = crate::sampler::collect_process_samples(&baseline.processes);

    std::thread::sleep(interval);

    let mut snapshot = sampler.sample(Some(options.pid));
    crate::sampler::apply_process_trends(&mut snapshot.processes, &previous);

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
    let limit = if options.full {
        usize::MAX
    } else {
        options.limit
    };
    let process = &inspection.process;
    let _ = writeln!(
        out,
        "{} pid {} | ppid {} | user {} | status {} | priority {}",
        process.name,
        process.pid,
        process
            .parent_pid
            .map(|pid| pid.to_string())
            .unwrap_or_else(|| "-".to_string()),
        process.user,
        process.status,
        process
            .priority
            .map(|priority| priority.to_string())
            .unwrap_or_else(|| "-".to_string())
    );
    let _ = writeln!(
        out,
        "CPU {} | Memory {} | Virtual {} | Mem% {} | runtime {} | impact {}",
        format::percent(process.cpu_usage_percent as f64),
        format::bytes(process.memory_bytes),
        format::bytes(process.virtual_memory_bytes),
        format::percent(process.memory_percent),
        format::duration(process.runtime_seconds),
        format::number(process.energy_impact),
    );
    let _ = writeln!(
        out,
        "Disk R {} | W {} | Total R {} | W {}",
        format::bytes_rate(process.disk_read_bytes_per_sec),
        format::bytes_rate(process.disk_write_bytes_per_sec),
        format::bytes(process.total_disk_read_bytes),
        format::bytes(process.total_disk_written_bytes),
    );
    let _ = writeln!(
        out,
        "Network In {} | Out {}",
        process
            .network_in_bytes_per_sec
            .map(format::bytes_rate)
            .unwrap_or_else(|| "-".to_string()),
        process
            .network_out_bytes_per_sec
            .map(format::bytes_rate)
            .unwrap_or_else(|| "-".to_string()),
    );
    if process.total_network_read_bytes.is_none() && process.total_network_written_bytes.is_none() {
        let _ = writeln!(
            out,
            "Total network traffic unavailable at process level in this sample"
        );
    } else {
        let _ = writeln!(
            out,
            "Total Network In {} | Out {}",
            process
                .total_network_read_bytes
                .map(format::bytes)
                .unwrap_or_else(|| "-".to_string()),
            process
                .total_network_written_bytes
                .map(format::bytes)
                .unwrap_or_else(|| "-".to_string()),
        );
    }
    let _ = writeln!(
        out,
        "started {} | session {} | threads {} | open files {}",
        format::epoch_time(process.start_time_unix),
        process
            .session_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "-".to_string()),
        process
            .thread_count
            .map(|count| count.to_string())
            .unwrap_or_else(|| "-".to_string()),
        process
            .open_files
            .map(|files| {
                process
                    .open_files_limit
                    .map(|limit| format!("{files}/{limit}"))
                    .unwrap_or_else(|| files.to_string())
            })
            .unwrap_or_else(|| "-".to_string()),
    );
    let _ = writeln!(out, "cwd: {}", process.cwd);
    let _ = writeln!(out, "exe: {}", process.executable);
    let _ = writeln!(out, "cmd: {}", process.command);

    let _ = writeln!(out);
    let _ = writeln!(out, "Sockets");
    if inspection.sockets.is_empty() {
        let _ = writeln!(out, "  none visible");
    } else {
        for socket in inspection.sockets.iter().take(limit) {
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
        for file in inspection.files.iter().take(limit) {
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

pub fn collect_handles(pid: u32) -> Result<ProcessHandles> {
    ensure_lsof_available()?;
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

pub(crate) fn ensure_lsof_available() -> Result<()> {
    static LSOF_AVAILABLE: OnceLock<bool> = OnceLock::new();
    let available = *LSOF_AVAILABLE.get_or_init(|| {
        Command::new("which")
            .arg("lsof")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    });
    if available {
        Ok(())
    } else {
        Err(error::message(
            "lsof was not found in PATH. Install it or add it to PATH (on macOS it ships with the system at /usr/sbin/lsof), then retry.",
        ))
    }
}

fn is_empty_lsof_result(output: &Output) -> bool {
    // lsof returns 1 when no files are found for the PID
    output.status.code() == Some(1)
}

pub(crate) fn lsof_failure_message(kind: &str, output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stderr = stderr.trim();
    if stderr.contains("Permission denied") {
        return format!(
            "lsof {kind} failed: Permission denied. Try running with sudo or granting 'Full Disk Access' to your terminal."
        );
    }
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
