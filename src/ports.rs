use std::{fmt::Write as _, process::Command};

use serde::Serialize;

use crate::{
    error::{self, Result},
    format,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PortOptions {
    pub port: Option<u16>,
    pub json: bool,
    pub all: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PortEntry {
    pub pid: u32,
    pub command: String,
    pub user: String,
    pub fd: String,
    pub protocol: String,
    pub local: String,
    pub remote: Option<String>,
    pub state: Option<String>,
}

pub fn lookup(options: PortOptions) -> Result<Vec<PortEntry>> {
    let mut args = vec!["-nP".to_string(), "-F".to_string(), "pPcLfnT".to_string()];
    match (options.port, options.all) {
        (Some(port), true) => {
            args.push(format!("-iTCP:{port}"));
            args.push(format!("-iUDP:{port}"));
        }
        (Some(port), false) => {
            args.push(format!("-iTCP:{port}"));
            args.push("-sTCP:LISTEN".to_string());
        }
        (None, true) => {
            args.push("-iTCP".to_string());
            args.push("-iUDP".to_string());
        }
        (None, false) => {
            args.push("-iTCP".to_string());
            args.push("-sTCP:LISTEN".to_string());
        }
    }

    let output = Command::new("lsof").args(&args).output()?;
    if !output.status.success() && output.stdout.is_empty() {
        if output.status.code() == Some(1) {
            return Ok(Vec::new());
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(error::message(format!("lsof failed: {}", stderr.trim())));
    }

    Ok(parse_lsof_fields(&String::from_utf8_lossy(&output.stdout)))
}

pub fn render(entries: &[PortEntry], options: PortOptions) -> Result<String> {
    if options.json {
        return Ok(serde_json::to_string_pretty(entries)?);
    }

    let mut out = String::new();
    let scope = match (options.port, options.all) {
        (Some(port), true) => format!("all sockets on port {port}"),
        (Some(port), false) => format!("listening TCP sockets on port {port}"),
        (None, true) => "all TCP/UDP sockets".to_string(),
        (None, false) => "listening TCP sockets".to_string(),
    };
    let _ = writeln!(out, "{scope}");
    if entries.is_empty() {
        let _ = writeln!(out, "No matching sockets found.");
        return Ok(out);
    }
    let _ = writeln!(
        out,
        "{:>7} {:<20} {:<13} {:<5} {:<5} {:<26} REMOTE/STATE",
        "PID", "COMMAND", "USER", "FD", "PROTO", "LOCAL"
    );
    for entry in entries {
        let remote_or_state = entry
            .remote
            .as_deref()
            .or(entry.state.as_deref())
            .unwrap_or("-");
        let _ = writeln!(
            out,
            "{:>7} {:<20} {:<13} {:<5} {:<5} {:<26} {}",
            entry.pid,
            format::truncate_middle(&entry.command, 20),
            format::truncate_middle(&entry.user, 13),
            format::truncate_middle(&entry.fd, 5),
            entry.protocol,
            format::truncate_middle(&entry.local, 26),
            remote_or_state,
        );
    }
    Ok(out)
}

fn parse_lsof_fields(output: &str) -> Vec<PortEntry> {
    let mut entries = Vec::new();
    let mut process = ProcessFields::default();
    let mut socket = SocketFields::default();

    for line in output.lines().filter(|line| !line.is_empty()) {
        let (field, value) = line.split_at(1);
        match field {
            "p" => {
                flush_socket(&process, &mut socket, &mut entries);
                process = ProcessFields {
                    pid: value.parse().ok(),
                    ..ProcessFields::default()
                };
            }
            "c" => process.command = value.to_string(),
            "L" => process.user = value.to_string(),
            "f" => {
                flush_socket(&process, &mut socket, &mut entries);
                socket.fd = value.to_string();
            }
            "P" => socket.protocol = value.to_string(),
            "n" => {
                let (local, remote) = value
                    .split_once("->")
                    .map(|(local, remote)| (local.to_string(), Some(remote.to_string())))
                    .unwrap_or_else(|| (value.to_string(), None));
                socket.local = local;
                socket.remote = remote;
            }
            "T" => {
                if let Some(state) = value.strip_prefix("ST=") {
                    socket.state = Some(state.to_string());
                }
            }
            _ => {}
        }
    }
    flush_socket(&process, &mut socket, &mut entries);
    entries
}

fn flush_socket(process: &ProcessFields, socket: &mut SocketFields, entries: &mut Vec<PortEntry>) {
    let Some(pid) = process.pid else {
        *socket = SocketFields::default();
        return;
    };
    if socket.fd.is_empty() || socket.local.is_empty() {
        *socket = SocketFields::default();
        return;
    }

    entries.push(PortEntry {
        pid,
        command: value_or_dash(&process.command),
        user: value_or_dash(&process.user),
        fd: socket.fd.clone(),
        protocol: value_or_dash(&socket.protocol),
        local: socket.local.clone(),
        remote: socket.remote.clone(),
        state: socket.state.clone(),
    });
    *socket = SocketFields::default();
}

fn value_or_dash(value: &str) -> String {
    if value.is_empty() {
        "-".to_string()
    } else {
        value.to_string()
    }
}

#[derive(Default)]
struct ProcessFields {
    pid: Option<u32>,
    command: String,
    user: String,
}

#[derive(Default)]
struct SocketFields {
    fd: String,
    protocol: String,
    local: String,
    remote: Option<String>,
    state: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::parse_lsof_fields;

    #[test]
    fn parses_lsof_field_output_into_socket_entries() {
        let entries = parse_lsof_fields(
            "\
p4286
cnode
Lmiloevans
f18
PTCP
n127.0.0.1:18789
TST=LISTEN
f24
PUDP
n[::1]:5353->[ff02::fb]:5353
",
        );

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].pid, 4286);
        assert_eq!(entries[0].command, "node");
        assert_eq!(entries[0].protocol, "TCP");
        assert_eq!(entries[0].local, "127.0.0.1:18789");
        assert_eq!(entries[0].state.as_deref(), Some("LISTEN"));
        assert_eq!(entries[1].protocol, "UDP");
        assert_eq!(entries[1].remote.as_deref(), Some("[ff02::fb]:5353"));
    }
}
