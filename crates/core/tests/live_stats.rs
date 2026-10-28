//! Takes two real samples on this machine and checks the numbers are sane.

use std::thread::sleep;
use std::time::Duration;

use tidewise_core::stats::LiveStats;

#[test]
fn two_real_samples_are_sane() {
    let mut stats = LiveStats::new();

    let first = stats.sample();
    assert_eq!(first.disk_read_per_sec, None, "first sample has no rates");
    assert_eq!(first.net_down_per_sec, None);

    sleep(Duration::from_millis(1200));
    let second = stats.sample();

    assert!(
        (0.0..=100.0).contains(&second.cpu_percent),
        "{}",
        second.cpu_percent
    );
    assert!(second.memory_total > 0);
    assert!(second.memory_used > 0 && second.memory_used <= second.memory_total);
    for rate in [
        second.disk_read_per_sec,
        second.disk_write_per_sec,
        second.net_down_per_sec,
        second.net_up_per_sec,
    ] {
        let rate = rate.expect("second sample has rates");
        assert!(rate.is_finite() && rate >= 0.0, "{rate}");
    }

    assert!(!second.processes.is_empty());
    let me = std::process::id();
    let mine = second
        .processes
        .iter()
        .find(|p| p.pid == me)
        .expect("this test process is listed");
    assert!(!mine.name.is_empty());
    assert!(mine.memory_bytes > 0);
    assert!(mine.cpu_percent >= 0.0);
}

/// Threads must not be listed as processes (sysinfo reports each thread with the parent's memory).
#[test]
fn threads_are_not_listed_as_processes() {
    use std::sync::{Arc, Barrier};
    let barrier = Arc::new(Barrier::new(5));
    let tids = Arc::new(std::sync::Mutex::new(Vec::new()));
    let handles: Vec<_> = (0..4)
        .map(|i| {
            let (b, t) = (barrier.clone(), tids.clone());
            std::thread::Builder::new()
                .name(format!("tw-worker-{i}"))
                .spawn(move || {
                    let tid = std::fs::read_link("/proc/thread-self")
                        .ok()
                        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                        .and_then(|n| n.parse::<u32>().ok());
                    t.lock().unwrap().extend(tid);
                    b.wait(); // keep the thread alive while we sample
                    b.wait();
                })
                .unwrap()
        })
        .collect();
    barrier.wait();

    let snapshot = LiveStats::new().sample();
    let tids = tids.lock().unwrap().clone();
    barrier.wait();
    handles.into_iter().for_each(|h| h.join().unwrap());

    assert_eq!(tids.len(), 4);
    for p in &snapshot.processes {
        assert!(
            !tids.contains(&p.pid),
            "thread {} listed as a process",
            p.pid
        );
        assert!(!p.name.starts_with("tw-worker"), "{}", p.name);
    }
}

/// On this real machine: no row above 100 % CPU, and private memory of all rows fits in RAM.
#[test]
fn rows_never_exceed_the_machine() {
    let mut stats = LiveStats::new();
    stats.sample();
    // Burn a little CPU on several threads so per-core values would exceed 100 %.
    let burners: Vec<_> = (0..4)
        .map(|_| {
            std::thread::spawn(|| {
                let end = std::time::Instant::now() + Duration::from_millis(1300);
                while std::time::Instant::now() < end {
                    std::hint::black_box(1u64.wrapping_mul(7));
                }
            })
        })
        .collect();
    sleep(Duration::from_millis(1200));
    let s = stats.sample();
    burners.into_iter().for_each(|h| h.join().unwrap());

    let total: u64 = s.processes.iter().map(|p| p.memory_bytes).sum();
    assert!(total <= s.memory_total, "{total} > {}", s.memory_total);
    for p in &s.processes {
        assert!(
            (0.0..=100.0).contains(&p.cpu_percent),
            "{} {}%",
            p.name,
            p.cpu_percent
        );
    }
}
