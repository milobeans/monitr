use std::{
    ffi::OsStr,
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
}

#[derive(Debug, Clone)]
pub struct ProcessRow {
    pub pid: u32,
    pub parent_pid: Option<u32>,
    pub name: String,
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
    pub run_time: u64,
    pub start_time: u64,
    pub energy_impact: f64,
    pub selected_details: Option<SelectedProcessDetails>,
    pub search_text: String,
}

#[derive(Debug, Clone)]
pub struct SelectedProcessDetails {
    pub thread_count: Option<usize>,
    pub open_files: Option<usize>,
    pub open_files_limit: Option<usize>,
    pub session_id: Option<u32>,
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

        let total_memory = self.system.total_memory();
        let processes = self
            .system
            .processes()
            .values()
            .map(|process| self.process_row(process, total_memory, detail_pid, seconds))
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

    fn process_row(
        &self,
        process: &Process,
        total_memory: u64,
        detail_pid: Option<u32>,
        seconds: f64,
    ) -> ProcessRow {
        let disk = process.disk_usage();
        let pid = process.pid().as_u32();
        let name = os_to_string(process.name());
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
        });
        let parent_pid = process.parent().map(|pid| pid.as_u32());
        let status = status_label(process.status()).to_string();
        let search_text = format!(
            "{} {} {} {} {}",
            pid,
            name.to_lowercase(),
            user.to_lowercase(),
            command.to_lowercase(),
            status.to_lowercase()
        );

        ProcessRow {
            pid,
            parent_pid,
            name,
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
            run_time: process.run_time(),
            start_time: process.start_time(),
            energy_impact,
            selected_details,
            search_text,
        }
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

    #[cfg(not(target_os = "macos"))]
    pub fn thread_count(_pid: u32) -> Option<usize> {
        None
    }
}
