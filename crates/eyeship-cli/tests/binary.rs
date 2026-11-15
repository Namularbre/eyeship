//! The workspace builds a binary named `eyeship` that runs successfully.

use std::process::Command;

#[test]
fn eyeship_binary_runs_successfully() {
    let status = Command::new(env!("CARGO_BIN_EXE_eyeship"))
        .status()
        .expect("the eyeship binary should start");

    assert!(status.success());
}
