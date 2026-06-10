use serde::Serialize;

use crate::sampler::ProcessRow;

#[derive(Debug, Clone, Serialize)]
pub struct ProcessRecord {
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
    pub memory_percent: f64,
    pub disk_read_bytes_per_sec: f64,
    pub disk_write_bytes_per_sec: f64,
    pub total_disk_read_bytes: u64,
    pub total_disk_written_bytes: u64,
    pub network_in_bytes_per_sec: Option<f64>,
    pub network_out_bytes_per_sec: Option<f64>,
    pub total_network_read_bytes: Option<u64>,
    pub total_network_written_bytes: Option<u64>,
    pub runtime_seconds: u64,
    pub start_time_unix: u64,
    pub energy_impact: f64,
    pub thread_count: Option<usize>,
    pub open_files: Option<usize>,
    pub open_files_limit: Option<usize>,
    pub session_id: Option<u32>,
    pub priority: Option<i32>,
}

impl From<&ProcessRow> for ProcessRecord {
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
            memory_percent: process.memory_percent,
            disk_read_bytes_per_sec: process.disk_read_rate,
            disk_write_bytes_per_sec: process.disk_write_rate,
            total_disk_read_bytes: process.total_disk_read,
            total_disk_written_bytes: process.total_disk_write,
            network_in_bytes_per_sec: process.network_in_rate,
            network_out_bytes_per_sec: process.network_out_rate,
            total_network_read_bytes: process.total_network_in,
            total_network_written_bytes: process.total_network_out,
            runtime_seconds: process.run_time,
            start_time_unix: process.start_time,
            energy_impact: process.energy_impact,
            thread_count: details.and_then(|details| details.thread_count),
            open_files: details.and_then(|details| details.open_files),
            open_files_limit: details.and_then(|details| details.open_files_limit),
            session_id: details.and_then(|details| details.session_id),
            priority: details.and_then(|details| details.priority),
        }
    }
}
