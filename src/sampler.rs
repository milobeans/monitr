use std::{
    collections::HashMap,
    ffi::OsStr,
    process::Command,
    time::{Duration, Instant},
};

use sysinfo::{
    CpuRefreshKind, DiskRefreshKind, Disks, Networks, Pid, Process, ProcessRefreshKind,
    ProcessStatus, ProcessesToUpdate, Signal, System, UpdateKind, Users,
};

use crate::error::{self, Result};

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub totals: SystemTotals,
    pub processes: Vec<ProcessRow>,
    pub disks: Vec<DiskRow>,
    pub networks: Vec<NetworkRow>,
    pub sample_span: Duration,
    pub process_count: usize,
}

#[derive(Debug, Clone)]
pub struct UsageSample {
    pub cpu_usage: f32,
    pub memory_percent: f64,
}

#[derive(Debug, Clone)]
pub struct SystemTotals {
    pub cpu_usage: f32,
    pub cpu_count: usize,
    pub total_memory: u64,
    pub used_memory: u64,
    pub total_swap: u64,
    pub used_swap: u64,
    pub disk_read_rate: f64,
    pub disk_write_rate: f64,
    pub net_in_rate: f64,
    pub net_out_rate: f64,
    pub uptime: u64,
    pub host: String,
    pub os: String,
    pub process_network_supported: bool,
    pub process_network_error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProcessRow {
    pub pid: u32,
    pub pid_str: String,
    pub parent_pid: Option<u32>,
    pub name: String,
    pub sort_name: String,
    pub user: String,
    pub command: String,
    pub exe: String,
    pub cwd: String,
    pub status: String,
    pub cpu_usage: f32,
    pub memory: u64,
    pub virtual_memory: u64,
    pub memory_percent: f64,
    pub disk_read_rate: f64,
    pub disk_write_rate: f64,
    pub total_disk_read: u64,
    pub total_disk_write: u64,
    pub network_in_rate: Option<f64>,
    pub network_out_rate: Option<f64>,
    pub total_network_in: Option<u64>,
    pub total_network_out: Option<u64>,
    pub network_attribution_supported: bool,
    pub run_time: u64,
    pub start_time: u64,
    pub energy_impact: f64,
    pub trend: ProcessTrend,
    pub selected_details: Option<SelectedProcessDetails>,
    pub search_text: String,
}

#[derive(Debug, Clone, Default)]
pub struct ProcessTrend {
    pub cpu_delta: f32,
    pub memory_delta: i64,
    pub disk_read_rate_delta: f64,
    pub disk_write_rate_delta: f64,
    pub network_in_rate_delta: f64,
    pub network_out_rate_delta: f64,
    pub new_process: bool,
}

impl ProcessTrend {
    pub fn disk_rate_delta(&self) -> f64 {
        self.disk_read_rate_delta + self.disk_write_rate_delta
    }

    pub fn network_rate_delta(&self) -> f64 {
        self.network_in_rate_delta + self.network_out_rate_delta
    }

    pub fn score(&self) -> f64 {
        let memory_mib = self.memory_delta.unsigned_abs() as f64 / 1_048_576.0;
        let disk_mib = self.disk_rate_delta().abs() / 1_048_576.0;
        let network_mib = self.network_rate_delta().abs() / 1_048_576.0;
        self.cpu_delta.abs() as f64
            + memory_mib.min(100.0)
            + disk_mib.min(100.0)
            + network_mib.min(100.0)
    }

    pub fn headline(&self) -> Option<String> {
        if self.new_process {
            return Some("new process".to_string());
        }

        let cpu_magnitude = self.cpu_delta.abs() as f64;
        let memory_mib = self.memory_delta.unsigned_abs() as f64 / 1_048_576.0;
        let disk_mib = self.disk_rate_delta().abs() / 1_048_576.0;
        let network_mib = self.network_rate_delta().abs() / 1_048_576.0;

        if cpu_magnitude
            .max(memory_mib)
            .max(disk_mib)
            .max(network_mib)
            < 0.05
        {
            return None;
        }

        if cpu_magnitude >= memory_mib && cpu_magnitude >= disk_mib && cpu_magnitude >= network_mib {
            Some(format!(
                "CPU {}",
                crate::format::signed_percent(self.cpu_delta as f64)
            ))
        } else if memory_mib >= disk_mib && memory_mib >= network_mib {
            Some(format!(
                "mem {}",
                crate::format::signed_bytes(self.memory_delta)
            ))
        } else if network_mib >= disk_mib {
            Some(format!(
                "net {}",
                crate::format::signed_bytes_rate(self.network_rate_delta())
            ))
        } else {
            Some(format!(
                "disk {}",
                crate::format::signed_bytes_rate(self.disk_rate_delta())
            ))
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProcessSample {
    cpu_usage: f32,
    memory: u64,
    disk_read_rate: f64,
    disk_write_rate: f64,
    network_in_rate: Option<f64>,
    network_out_rate: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct SelectedProcessDetails {
    pub thread_count: Option<usize>,
    pub open_files: Option<usize>,
    pub open_files_limit: Option<usize>,
    pub session_id: Option<u32>,
    pub priority: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct DiskRow {
    pub name: String,
    pub mount_point: String,
    pub total: u64,
    pub available: u64,
    pub read_rate: f64,
    pub write_rate: f64,
}

#[derive(Debug, Clone)]
pub struct NetworkRow {
    pub name: String,
    pub received_rate: f64,
    pub transmitted_rate: f64,
    pub total_received: u64,
    pub total_transmitted: u64,
}

pub struct Sampler {
    system: System,
    disks: Disks,
    networks: Networks,
    users: Users,
    last_sample: Instant,
    static_info: StaticInfo,
    previous_process_network_totals: HashMap<u32, ProcessNetworkTotals>,
}

#[derive(Debug, Clone, Copy)]
struct ProcessNetworkTotals {
    total_in: u64,
    total_out: u64,
}

#[derive(Debug, Clone, Copy)]
struct ProcessNetworkSample {
    totals: ProcessNetworkTotals,
    in_rate: f64,
    out_rate: f64,
}

#[derive(Debug, Clone)]
struct StaticInfo {
    host: String,
    os: String,
}

impl Sampler {
    pub fn new() -> Result<Self> {
        let mut system = System::new();
        system.refresh_cpu_specifics(CpuRefreshKind::everything());
        system.refresh_memory();
        system.refresh_processes_specifics(ProcessesToUpdate::All, true, process_refresh_kind());

        let disks = Disks::new_with_refreshed_list_specifics(DiskRefreshKind::everything());
        let networks = Networks::new_with_refreshed_list();
        let users = Users::new_with_refreshed_list();

        Ok(Self {
            system,
            disks,
            networks,
            users,
            last_sample: Instant::now(),
            static_info: StaticInfo {
                host: System::host_name().unwrap_or_else(|| "localhost".to_string()),
                os: System::long_os_version()
                    .or_else(System::name)
                    .unwrap_or_else(|| std::env::consts::OS.to_string()),
            },
            previous_process_network_totals: HashMap::new(),
        })
    }

    pub fn sample(&mut self, detail_pid: Option<u32>) -> Snapshot {
        let now = Instant::now();
        let sample_span = now.saturating_duration_since(self.last_sample);
        self.last_sample = now;
        let seconds = sample_span.as_secs_f64().max(0.25);

        self.system
            .refresh_cpu_specifics(CpuRefreshKind::nothing().with_cpu_usage());
        self.system.refresh_memory();
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            process_refresh_kind(),
        );
        self.disks
            .refresh_specifics(true, DiskRefreshKind::everything());
        self.networks.refresh(true);

        let (network_samples, network_attribution_supported, network_attribution_error) =
            self.collect_process_network_samples(seconds);

        let total_memory = self.system.total_memory();
        let processes = self
            .system
            .processes()
            .values()
            .map(|process| {
                let pid = process.pid().as_u32();
                self.process_row(
                    process,
                    total_memory,
                    detail_pid,
                    seconds,
                    network_samples.get(&pid).copied(),
                    network_attribution_supported,
                )
            })
            .collect::<Vec<_>>();

        let (disk_read_rate, disk_write_rate, disks) = self.disk_rows(seconds);
        let (net_in_rate, net_out_rate, networks) = self.network_rows(seconds);

        Snapshot {
            totals: SystemTotals {
                cpu_usage: self.system.global_cpu_usage(),
                cpu_count: self.system.cpus().len(),
                total_memory,
                used_memory: self.system.used_memory(),
                total_swap: self.system.total_swap(),
                used_swap: self.system.used_swap(),
                disk_read_rate,
                disk_write_rate,
                net_in_rate,
                net_out_rate,
                process_network_supported: network_attribution_supported,
                process_network_error: network_attribution_error,
                uptime: System::uptime(),
                host: self.static_info.host.clone(),
                os: self.static_info.os.clone(),
            },
            process_count: processes.len(),
            processes,
            disks,
            networks,
            sample_span,
        }
    }

    pub fn sample_usage(&mut self) -> UsageSample {
        let now = Instant::now();
        self.last_sample = now;

        self.system
            .refresh_cpu_specifics(CpuRefreshKind::nothing().with_cpu_usage());
        self.system.refresh_memory();

        let total_memory = self.system.total_memory();
        let memory_percent = if total_memory > 0 {
            self.system.used_memory() as f64 / total_memory as f64 * 100.0
        } else {
            0.0
        };

        UsageSample {
            cpu_usage: self.system.global_cpu_usage(),
            memory_percent,
        }
    }

    pub fn send_signal(&self, pid: u32, signal: Signal) -> Result<()> {
        let pid = Pid::from_u32(pid);
        let process = self
            .system
            .process(pid)
            .ok_or_else(|| error::message(format!("process {pid} is no longer visible")))?;
        match process.kill_with(signal) {
            Some(true) => Ok(()),
            Some(false) => Err(error::message(format!(
                "failed to send {signal} to pid {pid}"
            ))),
            None => Err(error::message(format!(
                "{signal} is not supported on this platform"
            ))),
        }
    }

    pub fn adjust_priority(&self, pid: u32, delta: i32) -> Result<i32> {
        let sys_pid = Pid::from_u32(pid);
        self.system
            .process(sys_pid)
            .ok_or_else(|| error::message(format!("process {sys_pid} is no longer visible")))?;
        let current = platform::priority(pid)
            .ok_or_else(|| error::message(format!("priority is unavailable for pid {pid}")))?;
        let next = (current + delta).clamp(-20, 20);
        platform::set_priority(pid, next)?;
        Ok(next)
    }

    pub fn selected_process_details(&mut self, pid: u32) -> Option<SelectedProcessDetails> {
        let pid = Pid::from_u32(pid);
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[pid]),
            false,
            process_refresh_kind(),
        );
        self.system
            .process(pid)
            .map(|process| SelectedProcessDetails {
                thread_count: platform::thread_count(process.pid().as_u32()),
                open_files: process.open_files(),
                open_files_limit: process.open_files_limit(),
                session_id: process.session_id().map(|pid| pid.as_u32()),
                priority: platform::priority(process.pid().as_u32()),
            })
    }

    fn process_row(
        &self,
        process: &Process,
        total_memory: u64,
        detail_pid: Option<u32>,
        seconds: f64,
        network_sample: Option<ProcessNetworkSample>,
        network_attribution_supported: bool,
    ) -> ProcessRow {
        let disk = process.disk_usage();
        let pid = process.pid().as_u32();
        let name = os_to_string(process.name());
        let sort_name = name.to_lowercase();
        let command = command_line(process);
        let user = process
            .user_id()
            .and_then(|id| self.users.get_user_by_id(id))
            .map(|user| user.name().to_string())
            .unwrap_or_else(|| "-".to_string());
        let exe = process
            .exe()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "-".to_string());
        let cwd = process
            .cwd()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "-".to_string());
        let memory = process.memory();
        let memory_percent = if total_memory > 0 {
            (memory as f64 / total_memory as f64) * 100.0
        } else {
            0.0
        };
        let (network_in_rate, network_out_rate, total_network_in, total_network_out) =
            network_sample
                .map(|sample| {
                    (
                        Some(sample.in_rate),
                        Some(sample.out_rate),
                        Some(sample.totals.total_in),
                        Some(sample.totals.total_out),
                    )
                })
                .unwrap_or((None, None, None, None));
        let disk_read_rate = disk.read_bytes as f64 / seconds;
        let disk_write_rate = disk.written_bytes as f64 / seconds;
        let energy_impact = energy_impact(
            process.cpu_usage(),
            memory_percent,
            disk_read_rate + disk_write_rate,
            process.status(),
        );
        let selected_details = (Some(pid) == detail_pid).then(|| SelectedProcessDetails {
            thread_count: platform::thread_count(pid),
            open_files: process.open_files(),
            open_files_limit: process.open_files_limit(),
            session_id: process.session_id().map(|pid| pid.as_u32()),
            priority: platform::priority(pid),
        });
        let parent_pid = process.parent().map(|pid| pid.as_u32());
        let status = status_label(process.status()).to_string();
        let search_text = format!(
            "{} {} {} {} {}",
            pid,
            sort_name,
            user.to_lowercase(),
            command.to_lowercase(),
            status.to_lowercase()
        );

        ProcessRow {
            pid,
            pid_str: pid.to_string(),
            parent_pid,
            name,
            sort_name,
            user,
            command,
            exe,
            cwd,
            status,
            cpu_usage: process.cpu_usage(),
            memory,
            virtual_memory: process.virtual_memory(),
            memory_percent,
            disk_read_rate,
            disk_write_rate,
            total_disk_read: disk.total_read_bytes,
            total_disk_write: disk.total_written_bytes,
            network_in_rate,
            network_out_rate,
            total_network_in,
            total_network_out,
            network_attribution_supported,
            run_time: process.run_time(),
            start_time: process.start_time(),
            energy_impact,
            trend: ProcessTrend::default(),
            selected_details,
            search_text,
        }
    }

    fn collect_process_network_samples(
        &mut self,
        seconds: f64,
    ) -> (HashMap<u32, ProcessNetworkSample>, bool, Option<String>) {
        let totals = match platform::process_network_totals() {
            Ok(totals) => totals,
            Err(error) => {
                self.previous_process_network_totals.clear();
                return (
                    HashMap::new(),
                    false,
                    Some(format!("process-level network attribution unavailable: {error}")),
                );
            }
        };

        let mut samples = HashMap::with_capacity(totals.len());
        for (pid, totals) in totals {
            let previous = self.previous_process_network_totals.get(&pid);
            let in_rate = previous
                .map(|previous| {
                    totals.total_in.saturating_sub(previous.total_in) as f64 / seconds
                })
                .unwrap_or(0.0);
            let out_rate = previous
                .map(|previous| {
                    totals.total_out.saturating_sub(previous.total_out) as f64 / seconds
                })
                .unwrap_or(0.0);

            samples.insert(
                pid,
                ProcessNetworkSample {
                    totals,
                    in_rate,
                    out_rate,
                },
            );
        }

        self.previous_process_network_totals = samples
            .iter()
            .map(|(&pid, sample)| (pid, sample.totals))
            .collect();

        (samples, true, None)
    }

    fn disk_rows(&self, seconds: f64) -> (f64, f64, Vec<DiskRow>) {
        let mut read_rate = 0.0;
        let mut write_rate = 0.0;
        let rows = self
            .disks
            .list()
            .iter()
            .map(|disk| {
                let usage = disk.usage();
                let row_read = usage.read_bytes as f64 / seconds;
                let row_write = usage.written_bytes as f64 / seconds;
                read_rate += row_read;
                write_rate += row_write;
                DiskRow {
                    name: os_to_string(disk.name()),
                    mount_point: disk.mount_point().display().to_string(),
                    total: disk.total_space(),
                    available: disk.available_space(),
                    read_rate: row_read,
                    write_rate: row_write,
                }
            })
            .collect();
        (read_rate, write_rate, rows)
    }

    fn network_rows(&self, seconds: f64) -> (f64, f64, Vec<NetworkRow>) {
        let mut in_rate = 0.0;
        let mut out_rate = 0.0;
        let rows = self
            .networks
            .iter()
            .map(|(name, data)| {
                let received_rate = data.received() as f64 / seconds;
                let transmitted_rate = data.transmitted() as f64 / seconds;
                in_rate += received_rate;
                out_rate += transmitted_rate;
                NetworkRow {
                    name: name.clone(),
                    received_rate,
                    transmitted_rate,
                    total_received: data.total_received(),
                    total_transmitted: data.total_transmitted(),
                }
            })
            .collect();
        (in_rate, out_rate, rows)
    }
}

pub fn collect_process_samples(processes: &[ProcessRow]) -> HashMap<u32, ProcessSample> {
    processes
        .iter()
        .map(|process| {
            (
                process.pid,
                ProcessSample {
                    cpu_usage: process.cpu_usage,
                    memory: process.memory,
                    disk_read_rate: process.disk_read_rate,
                    disk_write_rate: process.disk_write_rate,
                    network_in_rate: process.network_in_rate,
                    network_out_rate: process.network_out_rate,
                },
            )
        })
        .collect()
}

pub fn apply_process_trends(processes: &mut [ProcessRow], previous: &HashMap<u32, ProcessSample>) {
    for process in processes {
        process.trend = previous
            .get(&process.pid)
            .map(|sample| ProcessTrend {
                cpu_delta: process.cpu_usage - sample.cpu_usage,
                memory_delta: process.memory as i64 - sample.memory as i64,
                disk_read_rate_delta: process.disk_read_rate - sample.disk_read_rate,
                disk_write_rate_delta: process.disk_write_rate - sample.disk_write_rate,
                network_in_rate_delta: process.network_in_rate.unwrap_or(0.0)
                    - sample.network_in_rate.unwrap_or(0.0),
                network_out_rate_delta: process.network_out_rate.unwrap_or(0.0)
                    - sample.network_out_rate.unwrap_or(0.0),
                new_process: false,
            })
            .unwrap_or_else(|| ProcessTrend {
                new_process: true,
                ..ProcessTrend::default()
            });
    }
}

fn process_refresh_kind() -> ProcessRefreshKind {
    ProcessRefreshKind::nothing()
        .with_cpu()
        .with_memory()
        .with_disk_usage()
        .with_user(UpdateKind::OnlyIfNotSet)
        .with_cmd(UpdateKind::OnlyIfNotSet)
        .with_exe(UpdateKind::OnlyIfNotSet)
        .with_cwd(UpdateKind::OnlyIfNotSet)
        .without_tasks()
}

fn command_line(process: &Process) -> String {
    if process.cmd().is_empty() {
        return os_to_string(process.name());
    }
    process
        .cmd()
        .iter()
        .map(|part| part.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ")
}

fn os_to_string(value: &OsStr) -> String {
    let value = value.to_string_lossy();
    if value.is_empty() {
        "-".to_string()
    } else {
        value.into_owned()
    }
}

fn status_label(status: ProcessStatus) -> &'static str {
    match status {
        ProcessStatus::Idle => "idle",
        ProcessStatus::Run => "running",
        ProcessStatus::Sleep => "sleeping",
        ProcessStatus::Stop => "stopped",
        ProcessStatus::Zombie => "zombie",
        ProcessStatus::Tracing => "tracing",
        ProcessStatus::Dead => "dead",
        ProcessStatus::Wakekill => "wakekill",
        ProcessStatus::Waking => "waking",
        ProcessStatus::Parked => "parked",
        ProcessStatus::LockBlocked => "blocked",
        ProcessStatus::UninterruptibleDiskSleep => "disk sleep",
        ProcessStatus::Suspended => "suspended",
        ProcessStatus::Unknown(_) => "unknown",
    }
}

fn energy_impact(cpu: f32, memory_percent: f64, io_rate: f64, status: ProcessStatus) -> f64 {
    let activity = match status {
        ProcessStatus::Run => 1.0,
        ProcessStatus::Sleep | ProcessStatus::Idle => 0.0,
        _ => 0.25,
    };
    let io_mib = io_rate / 1_048_576.0;
    (cpu as f64 * 0.75) + (memory_percent * 0.2) + (io_mib * 0.35) + activity
}

mod platform {
    use std::collections::HashMap;

    #[cfg(target_os = "macos")]
    pub fn thread_count(pid: u32) -> Option<usize> {
        use std::{ffi::c_void, mem::size_of};

        const PROC_PIDTASKINFO: libc::c_int = 4;

        #[repr(C)]
        #[derive(Default)]
        struct ProcTaskInfo {
            pti_virtual_size: u64,
            pti_resident_size: u64,
            pti_total_user: u64,
            pti_total_system: u64,
            pti_threads_user: u64,
            pti_threads_system: u64,
            pti_policy: i32,
            pti_faults: i32,
            pti_pageins: i32,
            pti_cow_faults: i32,
            pti_messages_sent: i32,
            pti_messages_received: i32,
            pti_syscalls_mach: i32,
            pti_syscalls_unix: i32,
            pti_csw: i32,
            pti_threadnum: i32,
            pti_numrunning: i32,
            pti_priority: i32,
        }

        unsafe extern "C" {
            fn proc_pidinfo(
                pid: libc::c_int,
                flavor: libc::c_int,
                arg: u64,
                buffer: *mut c_void,
                buffersize: libc::c_int,
            ) -> libc::c_int;
        }

        let mut info = ProcTaskInfo::default();
        let size = size_of::<ProcTaskInfo>() as libc::c_int;
        let written = unsafe {
            proc_pidinfo(
                pid as libc::c_int,
                PROC_PIDTASKINFO,
                0,
                &mut info as *mut _ as *mut c_void,
                size,
            )
        };

        (written == size && info.pti_threadnum >= 0).then_some(info.pti_threadnum as usize)
    }

    #[cfg(target_os = "macos")]
    pub fn process_network_totals() -> Result<HashMap<u32, super::ProcessNetworkTotals>, String> {
        let output = Command::new("nettop")
            .args(["-L1", "-P", "-n", "-x"])
            .output()
            .map_err(|error| format!("failed to run nettop: {error}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let message = stderr.trim();
            if message.is_empty() {
                Err(format!(
                    "nettop exited with status {}",
                    output.status.code().map_or_else(
                        || "signal".to_string(),
                        |code| code.to_string()
                    )
                ))
            } else {
                Err(format!("nettop failed: {message}"))
            }
        } else {
            parse_process_network_totals(&output.stdout)
        }
    }

    #[cfg(target_os = "macos")]
    fn parse_process_network_totals(output: &[u8]) -> Result<HashMap<u32, super::ProcessNetworkTotals>, String> {
        let mut totals = HashMap::new();
        let stdout = String::from_utf8_lossy(output);
        for line in stdout.lines() {
            if line.trim().is_empty() {
                continue;
            }
            if line.starts_with("time,") {
                continue;
            }

            let fields: Vec<&str> = line.split(',').collect();
            let Some(process) = fields.get(1).copied() else {
                continue;
            };
            let Some(pid) = parse_pid(process) else {
                continue;
            };
            let bytes_in = fields
                .get(4)
                .and_then(|value| parse_u64(value))
                .unwrap_or(0);
            let bytes_out = fields
                .get(5)
                .and_then(|value| parse_u64(value))
                .unwrap_or(0);

            totals.insert(
                pid,
                super::ProcessNetworkTotals {
                    total_in: bytes_in,
                    total_out: bytes_out,
                },
            );
        }
        Ok(totals)
    }

    fn parse_u64(raw: &str) -> Option<u64> {
        let value = raw.trim().replace(',', "");
        value.parse::<u64>().ok()
    }

    #[cfg(target_os = "macos")]
    fn parse_pid(process: &str) -> Option<u32> {
        let (_, pid) = process.rsplit_once('.')?;
        pid.parse().ok()
    }

    #[cfg(target_os = "macos")]
    pub fn priority(pid: u32) -> Option<i32> {
        unsafe {
            *libc::__error() = 0;
            let value = libc::getpriority(libc::PRIO_PROCESS, pid as libc::id_t);
            (*libc::__error() == 0).then_some(value)
        }
    }

    #[cfg(target_os = "macos")]
    pub fn set_priority(pid: u32, priority: i32) -> std::io::Result<()> {
        let result = unsafe { libc::setpriority(libc::PRIO_PROCESS, pid as libc::id_t, priority) };
        if result == 0 {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error())
        }
    }

    #[cfg(not(target_os = "macos"))]
    pub fn thread_count(_pid: u32) -> Option<usize> {
        None
    }

    #[cfg(not(target_os = "macos"))]
    pub fn priority(_pid: u32) -> Option<i32> {
        None
    }

    #[cfg(not(target_os = "macos"))]
    pub fn set_priority(_pid: u32, _priority: i32) -> std::io::Result<()> {
        Err(std::io::Error::other(
            "priority control is not supported on this platform",
        ))
    }

    #[cfg(not(target_os = "macos"))]
    pub fn process_network_totals() -> Result<HashMap<u32, super::ProcessNetworkTotals>, String> {
        Err("process-level network attribution is not supported on this platform".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{ProcessRow, ProcessTrend, apply_process_trends, collect_process_samples};

    #[test]
    fn process_trends_compare_against_previous_sample() {
        let previous = vec![process(1, 10.0, 1_000, 100.0, 200.0)];
        let previous = collect_process_samples(&previous);
        let mut current = vec![
            process(1, 25.0, 1_500, 250.0, 150.0),
            process(2, 5.0, 500, 0.0, 0.0),
        ];

        apply_process_trends(&mut current, &previous);

        assert_eq!(current[0].trend.cpu_delta, 15.0);
        assert_eq!(current[0].trend.memory_delta, 500);
        assert_eq!(current[0].trend.disk_read_rate_delta, 150.0);
        assert_eq!(current[0].trend.disk_write_rate_delta, -50.0);
        assert!(!current[0].trend.new_process);
        assert!(current[1].trend.new_process);
    }

    #[test]
    fn trend_headline_names_the_dominant_driver() {
        let new_process = ProcessTrend {
            new_process: true,
            ..ProcessTrend::default()
        };
        assert_eq!(new_process.headline().as_deref(), Some("new process"));

        let cpu_heavy = ProcessTrend {
            cpu_delta: 35.0,
            memory_delta: 1_048_576,
            ..ProcessTrend::default()
        };
        assert_eq!(cpu_heavy.headline().as_deref(), Some("CPU +35.0%"));

        let memory_heavy = ProcessTrend {
            cpu_delta: 1.0,
            memory_delta: 200_000_000,
            ..ProcessTrend::default()
        };
        assert_eq!(memory_heavy.headline().as_deref(), Some("mem +200 MB"));

        let idle = ProcessTrend::default();
        assert_eq!(idle.headline(), None);
    }

    fn process(
        pid: u32,
        cpu_usage: f32,
        memory: u64,
        disk_read_rate: f64,
        disk_write_rate: f64,
    ) -> ProcessRow {
        ProcessRow {
            pid,
            pid_str: pid.to_string(),
            parent_pid: None,
            name: format!("process-{pid}"),
            sort_name: format!("process-{pid}"),
            user: "user".to_string(),
            command: "command".to_string(),
            exe: "-".to_string(),
            cwd: "-".to_string(),
            status: "running".to_string(),
            cpu_usage,
            memory,
            virtual_memory: memory,
            memory_percent: 0.0,
            disk_read_rate,
            disk_write_rate,
            total_disk_read: 0,
            total_disk_write: 0,
            network_in_rate: None,
            network_out_rate: None,
            total_network_in: None,
            total_network_out: None,
            network_attribution_supported: false,
            run_time: 0,
            start_time: 0,
            energy_impact: 0.0,
            trend: ProcessTrend::default(),
            selected_details: None,
            search_text: String::new(),
        }
    }
}
