#![cfg(target_os = "linux")]
use durandal_rs::metrics::LiveSampler;

#[test]
fn live_sampler_reads_linux_proc_and_primes_interval_counters() {
    let mut sampler = LiveSampler::new();
    let first = sampler.collect().expect("Linux sample");
    assert!(first.snapshot.memory.total_bytes > 0);
    assert!(!first.process_cpu_available);
    let second = sampler.collect().expect("second Linux sample");
    assert!(second.process_cpu_available);
    assert!(
        second
            .snapshot
            .top_processes
            .iter()
            .any(|p| p.pid == std::process::id() as i32)
    );
    assert!(
        second
            .snapshot
            .top_processes
            .iter()
            .all(|p| p.cpu_percent.is_finite() && p.cpu_percent >= 0.0)
    );
}
