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
