//! Runs the shell tests for `spikes/measure.sh` so `cargo test --all` covers them.

use std::path::Path;
use std::process::Command;

#[test]
fn measure_script_summing_logic() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new("/bin/bash")
        .arg(root.join("tests/measure/test_measure.sh"))
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("ok   pss of the whole tree"), "{stdout}");
}
