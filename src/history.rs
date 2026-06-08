use std::collections::{HashMap, HashSet, VecDeque};

use crate::{format, sampler::Snapshot};

/// Number of samples retained per series. At the default 1s refresh this is two
/// minutes of history; the bound keeps memory flat regardless of session length.
const CAPACITY: usize = 120;

/// Rolling time-series of system and per-process metrics, used to render
/// sparklines. Per-process series are pruned to live PIDs on every record so
/// memory stays proportional to the current process count, not session age.
#[derive(Default)]
pub struct History {
    cpu: VecDeque<f64>,
    memory: VecDeque<f64>,
    process_cpu: HashMap<u32, VecDeque<f64>>,
}

impl History {
    pub fn record(&mut self, snapshot: &Snapshot) {
        push_capped(&mut self.cpu, snapshot.totals.cpu_usage as f64);
        push_capped(&mut self.memory, memory_percent(snapshot));

        for process in &snapshot.processes {
            push_capped(
                self.process_cpu.entry(process.pid).or_default(),
                process.cpu_usage as f64,
            );
        }

        let live: HashSet<u32> = snapshot
            .processes
            .iter()
            .map(|process| process.pid)
            .collect();
        self.process_cpu.retain(|pid, _| live.contains(pid));
    }

    /// System CPU usage sparkline on a fixed 0-100% scale.
    pub fn cpu_sparkline(&self, width: usize) -> String {
        format::sparkline(&recent(&self.cpu, width), 100.0)
    }

    /// System memory usage sparkline on a fixed 0-100% scale.
    pub fn memory_sparkline(&self, width: usize) -> String {
        format::sparkline(&recent(&self.memory, width), 100.0)
    }

    /// Per-process CPU sparkline scaled against the window's own peak, so a
    /// single process's shape reads clearly regardless of core count. Returns
    /// `None` until at least two samples exist.
    pub fn process_cpu_sparkline(&self, pid: u32, width: usize) -> Option<String> {
        let series = self.process_cpu.get(&pid)?;
        if series.len() < 2 {
            return None;
        }
        let values = recent(series, width);
        let max = values.iter().copied().fold(1.0, f64::max);
        Some(format::sparkline(&values, max))
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

fn push_capped(series: &mut VecDeque<f64>, value: f64) {
    if series.len() == CAPACITY {
        series.pop_front();
    }
    series.push_back(value);
}

fn recent(series: &VecDeque<f64>, width: usize) -> Vec<f64> {
    let start = series.len().saturating_sub(width);
    series.iter().skip(start).copied().collect()
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
            run_time: 0,
            start_time: 0,
            energy_impact: 0.0,
            trend: ProcessTrend::default(),
            selected_details: None,
            search_text: String::new(),
        }
    }

    #[test]
    fn records_system_series_and_renders_sparklines() {
        let mut history = History::default();
        history.record(&snapshot(0.0, 0, Vec::new()));
        history.record(&snapshot(100.0, 100, Vec::new()));

        assert_eq!(history.cpu_sparkline(8), "▁█");
        assert_eq!(history.memory_sparkline(8), "▁█");
    }

    #[test]
    fn process_series_needs_two_samples_and_drops_dead_pids() {
        let mut history = History::default();
        history.record(&snapshot(0.0, 0, vec![process(1, 10.0)]));
        // One sample is not enough to draw a trend.
        assert!(history.process_cpu_sparkline(1, 8).is_none());

        history.record(&snapshot(0.0, 0, vec![process(1, 20.0)]));
        assert!(history.process_cpu_sparkline(1, 8).is_some());

        // PID 1 is gone next sample, so its series is pruned.
        history.record(&snapshot(0.0, 0, vec![process(2, 5.0)]));
        assert!(history.process_cpu_sparkline(1, 8).is_none());
    }

    #[test]
    fn series_length_is_bounded_by_capacity() {
        let mut history = History::default();
        for _ in 0..(CAPACITY + 50) {
            history.record(&snapshot(50.0, 50, vec![process(1, 50.0)]));
        }
        // recent() can never return more than the stored capacity.
        assert_eq!(
            history.cpu_sparkline(CAPACITY + 50).chars().count(),
            CAPACITY
        );
    }
}
