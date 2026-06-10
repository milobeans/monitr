use std::collections::{HashMap, HashSet};

use crate::{format, sampler::Snapshot};

const CAPACITY: usize = 120;

#[derive(Default)]
struct Series {
    values: Vec<f64>,
}

impl Series {
    fn push(&mut self, value: f64) {
        if self.values.len() == CAPACITY {
            self.values.remove(0);
        }
        self.values.push(value);
    }

    fn len(&self) -> usize {
        self.values.len()
    }

    fn recent(&self, width: usize) -> &[f64] {
        let start = self.values.len().saturating_sub(width);
        &self.values[start..]
    }
}

#[derive(Default)]
pub struct History {
    cpu: Series,
    memory: Series,
    process_cpu: HashMap<u32, Series>,
}

impl History {
    pub fn record(&mut self, snapshot: &Snapshot) {
        self.record_usage(snapshot.totals.cpu_usage as f64, memory_percent(snapshot));

        let mut live_pids: HashSet<u32> = HashSet::with_capacity(snapshot.processes.len());
        for process in &snapshot.processes {
            live_pids.insert(process.pid);
            self.process_cpu
                .entry(process.pid)
                .or_default()
                .push(process.cpu_usage as f64);
        }

        self.process_cpu.retain(|pid, _| live_pids.contains(pid));
    }

    pub fn record_usage(&mut self, cpu_usage: f64, memory_percent: f64) {
        self.cpu.push(cpu_usage);
        self.memory.push(memory_percent);
    }

    pub fn cpu_recent(&self, width: usize) -> Vec<f64> {
        self.cpu.recent(width).to_vec()
    }

    pub fn memory_recent(&self, width: usize) -> Vec<f64> {
        self.memory.recent(width).to_vec()
    }

    pub fn process_cpu_sparkline(&self, pid: u32, width: usize) -> Option<String> {
        let series = self.process_cpu.get(&pid)?;
        if series.len() < 2 {
            return None;
        }
        let values = series.recent(width);
        let max = values.iter().copied().fold(1.0, f64::max);
        Some(format::sparkline(values, max))
    }
}

fn memory_percent(snapshot: &Snapshot) -> f64 {
    let totals = &snapshot.totals;
    if totals.total_memory > 0 {
        totals.used_memory as f64 / totals.total_memory as f64 * 100.0
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::sampler::{ProcessRow, ProcessTrend, Snapshot, SystemTotals};

    use super::{CAPACITY, History};

    fn snapshot(cpu: f32, used_memory: u64, processes: Vec<ProcessRow>) -> Snapshot {
        Snapshot {
            totals: SystemTotals {
                cpu_usage: cpu,
                cpu_count: 8,
                total_memory: 100,
                used_memory,
                total_swap: 0,
                used_swap: 0,
                disk_read_rate: 0.0,
                disk_write_rate: 0.0,
                net_in_rate: 0.0,
                net_out_rate: 0.0,
                process_network_supported: false,
                process_network_error: None,
                uptime: 0,
                host: "host".into(),
                os: "macOS".into(),
            },
            process_count: processes.len(),
            processes,
            disks: Vec::new(),
            networks: Vec::new(),
            sample_span: Duration::from_millis(1_000),
        }
    }

    fn process(pid: u32, cpu_usage: f32) -> ProcessRow {
        ProcessRow {
            pid,
            pid_str: pid.to_string(),
            parent_pid: None,
            name: format!("process-{pid}"),
            sort_name: format!("process-{pid}"),
            user: "user".into(),
            command: "command".into(),
            exe: "-".into(),
            cwd: "-".into(),
            status: "running".into(),
            cpu_usage,
            memory: 0,
            virtual_memory: 0,
            memory_percent: 0.0,
            disk_read_rate: 0.0,
            disk_write_rate: 0.0,
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

    #[test]
    fn records_system_series_and_exposes_recent_values() {
        let mut history = History::default();
        history.record(&snapshot(0.0, 0, Vec::new()));
        history.record(&snapshot(100.0, 100, Vec::new()));

        assert_eq!(history.cpu_recent(8), vec![0.0, 100.0]);
        assert_eq!(history.memory_recent(8), vec![0.0, 100.0]);
    }

    #[test]
    fn records_usage_without_process_state() {
        let mut history = History::default();
        history.record_usage(25.0, 40.0);

        assert_eq!(history.cpu_recent(8), vec![25.0]);
        assert_eq!(history.memory_recent(8), vec![40.0]);
    }

    #[test]
    fn process_series_needs_two_samples_and_drops_dead_pids() {
        let mut history = History::default();
        history.record(&snapshot(0.0, 0, vec![process(1, 10.0)]));
        assert!(history.process_cpu_sparkline(1, 8).is_none());

        history.record(&snapshot(0.0, 0, vec![process(1, 20.0)]));
        assert!(history.process_cpu_sparkline(1, 8).is_some());

        history.record(&snapshot(0.0, 0, vec![process(2, 5.0)]));
        assert!(history.process_cpu_sparkline(1, 8).is_none());
    }

    #[test]
    fn series_length_is_bounded_by_capacity() {
        let mut history = History::default();
        for _ in 0..(CAPACITY + 50) {
            history.record(&snapshot(50.0, 50, vec![process(1, 50.0)]));
        }
        assert_eq!(history.cpu_recent(CAPACITY + 50).len(), CAPACITY);
    }
}
