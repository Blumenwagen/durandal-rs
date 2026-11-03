//! Interval measurements for the interactive dashboard. One-shot agent output stays unchanged.
use super::*;
use std::{collections::HashMap, time::Instant};

#[derive(Clone, Copy)]
struct ProcessCounter {
    started: u64,
    ticks: u64,
}

pub struct LiveSample {
    pub snapshot: Snapshot,
    pub collected_at: Instant,
    pub process_cpu_available: bool,
    pub network_rates_available: bool,
}

pub struct LiveSampler {
    previous_time: Option<Instant>,
    processes: HashMap<i32, ProcessCounter>,
    network: Option<(u64, u64)>,
    ticks_per_second: Option<f64>,
}

impl Default for LiveSampler {
    fn default() -> Self {
        Self::new()
    }
}

impl LiveSampler {
    pub fn new() -> Self {
        let ticks_per_second = Command::new("getconf")
            .arg("CLK_TCK")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| s.trim().parse::<f64>().ok())
            .filter(|v| v.is_finite() && *v > 0.0);
        Self {
            previous_time: None,
            processes: HashMap::new(),
            network: None,
            ticks_per_second,
        }
    }

    pub fn collect(&mut self) -> anyhow::Result<LiveSample> {
        anyhow::ensure!(
            Path::new("/proc/stat").exists(),
            "Live telemetry requires Linux with /proc mounted"
        );
        // Do not preselect by RSS: a CPU-heavy process must remain discoverable when sorting by CPU.
        let snapshot = collect_snapshot(usize::MAX)?;
        let counters = snapshot
            .top_processes
            .iter()
            .filter_map(|p| {
                let stat = fs::read_to_string(format!("/proc/{}/stat", p.pid)).ok()?;
                Some((p.pid, process_counter(&stat)?))
            })
            .collect();
        let network = fs::read_to_string("/proc/net/dev")
            .ok()
            .and_then(|s| network_counter(&s));
        Ok(self.apply(snapshot, counters, network, Instant::now()))
    }

    fn apply(
        &mut self,
        mut snapshot: Snapshot,
        counters: HashMap<i32, ProcessCounter>,
        network: Option<(u64, u64)>,
        now: Instant,
    ) -> LiveSample {
        let elapsed = self
            .previous_time
            .map(|t| now.saturating_duration_since(t).as_secs_f64())
            .filter(|v| *v > 0.0);
        let process_cpu_available =
            elapsed.is_some() && self.ticks_per_second.is_some() && !counters.is_empty();
        if let (Some(seconds), Some(hz)) = (elapsed, self.ticks_per_second) {
            for process in &mut snapshot.top_processes {
                process.cpu_percent =
                    match (self.processes.get(&process.pid), counters.get(&process.pid)) {
                        (Some(old), Some(new))
                            if old.started == new.started && new.ticks >= old.ticks =>
                        {
                            (new.ticks - old.ticks) as f64 / hz / seconds * 100.0
                        }
                        _ => 0.0, // New/reused PIDs need a baseline; never inherit the old process's CPU.
                    };
            }
        }
        let network_rates_available =
            elapsed.is_some() && self.network.is_some() && network.is_some();
        if let (Some(seconds), Some(old), Some(new)) = (elapsed, self.network, network) {
            snapshot.network.recv_bytes_per_sec =
                (new.0.saturating_sub(old.0) as f64 / seconds) as u64;
            snapshot.network.sent_bytes_per_sec =
                (new.1.saturating_sub(old.1) as f64 / seconds) as u64;
        }
        self.previous_time = Some(now);
        self.processes = counters;
        self.network = network;
        LiveSample {
            snapshot,
            collected_at: now,
            process_cpu_available,
            network_rates_available,
        }
    }
}

fn process_counter(stat: &str) -> Option<ProcessCounter> {
    let (_, tail) = stat.rsplit_once(')')?;
    let fields: Vec<_> = tail.split_whitespace().collect();
    Some(ProcessCounter {
        started: fields.get(19)?.parse().ok()?,
        ticks: fields
            .get(11)?
            .parse::<u64>()
            .ok()?
            .saturating_add(fields.get(12)?.parse::<u64>().ok()?),
    })
}

fn network_counter(text: &str) -> Option<(u64, u64)> {
    let mut total = (0u64, 0u64);
    let mut found = false;
    for line in text.lines() {
        let Some((interface, counters)) = line.split_once(':') else {
            continue;
        };
        if interface.trim() == "lo" {
            continue;
        }
        let fields: Vec<_> = counters.split_whitespace().collect();
        let (Some(rx), Some(tx)) = (
            fields.first().and_then(|s| s.parse::<u64>().ok()),
            fields.get(8).and_then(|s| s.parse::<u64>().ok()),
        ) else {
            continue;
        };
        total.0 = total.0.saturating_add(rx);
        total.1 = total.1.saturating_add(tx);
        found = true;
    }
    found.then_some(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn counter(started: u64, ticks: u64) -> HashMap<i32, ProcessCounter> {
        [(42, ProcessCounter { started, ticks })].into()
    }
    #[test]
    fn interval_rates_use_elapsed_time_and_do_not_inherit_reused_pids() {
        let mut sampler = LiveSampler {
            ticks_per_second: Some(100.0),
            previous_time: None,
            processes: HashMap::new(),
            network: None,
        };
        let now = Instant::now();
        let snapshot = Snapshot {
            top_processes: vec![ProcessInfo {
                pid: 42,
                ..Default::default()
            }],
            ..Default::default()
        };
        let first = sampler.apply(snapshot.clone(), counter(10, 100), Some((1000, 2000)), now);
        assert!(!first.process_cpu_available);
        assert!(!first.network_rates_available);
        let second = sampler.apply(
            snapshot.clone(),
            counter(10, 400),
            Some((5000, 3000)),
            now + Duration::from_secs(2),
        );
        assert_eq!(second.snapshot.top_processes[0].cpu_percent, 150.0);
        assert_eq!(second.snapshot.network.recv_bytes_per_sec, 2000);
        assert_eq!(second.snapshot.network.sent_bytes_per_sec, 500);
        assert!(second.process_cpu_available);
        assert!(second.network_rates_available);
        let reused = sampler.apply(
            snapshot,
            counter(20, 800),
            Some((10, 10)),
            now + Duration::from_secs(3),
        );
        assert_eq!(reused.snapshot.top_processes[0].cpu_percent, 0.0);
        assert_eq!(reused.snapshot.network.recv_bytes_per_sec, 0);
    }
    #[test]
    fn network_counters_exclude_loopback_and_bad_rows() {
        assert_eq!(
            network_counter(
                "Inter-| Receive\nlo: 1000 0 0 0 0 0 0 0 1000\neth0: 12 0 0 0 0 0 0 0 34\nveth: 7 0 0 0 0 0 0 0 8\nbad: nope"
            ),
            Some((19, 42))
        );
        assert_eq!(network_counter("lo: 100 0 0 0 0 0 0 0 100"), None);
    }
    #[test]
    fn process_counter_handles_spaces_and_parentheses_in_names() {
        let mut fields = vec!["0"; 22];
        fields[0] = "R";
        fields[11] = "20";
        fields[12] = "3";
        fields[19] = "456";
        let counter = process_counter(&format!("42 (worker (test)) {}", fields.join(" "))).unwrap();
        assert_eq!(counter.started, 456);
        assert_eq!(counter.ticks, 23);
    }
}
