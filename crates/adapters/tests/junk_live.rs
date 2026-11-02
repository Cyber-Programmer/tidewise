//! Looks at this machine's real junk, read-only. Opt-in: `cargo test -p tidewise-adapters --test
//! junk_live -- --ignored --nocapture`. It asserts nothing about the numbers (they are the machine's).

use tidewise_adapters::junk;

#[test]
#[ignore = "reads the real home folder and runs read-only commands; run by hand"]
fn real_machine_report() {
    let report = junk::scan(&junk::live_sources());
    for item in &report.items {
        println!(
            "{:>14} B  {:?}  {}{}",
            item.bytes,
            item.kind,
            item.label,
            if item.needs_admin { "  (admin)" } else { "" }
        );
    }
    for s in &report.skipped {
        println!("skipped {}: {}", s.label, s.reason);
    }
    println!(
        "user {} B, admin {} B",
        report.user_bytes(),
        report.admin_bytes()
    );
    assert!(report.items.windows(2).all(|w| w[0].bytes >= w[1].bytes));
}
