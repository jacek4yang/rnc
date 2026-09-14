#![cfg(windows)]

#[test]
fn real_windows_console() {
    let result = std::process::Command::new("python")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/windows_console_test.py"
        ))
        .args(["--exe", env!("CARGO_BIN_EXE_nc")])
        .output()
        .expect("Windows console tests require Python 3 (stdlib only)");
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn real_cmd_pipes_and_redirection() {
    let result = std::process::Command::new("python")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/windows_io_test.py"
        ))
        .args(["--exe", env!("CARGO_BIN_EXE_nc")])
        .output()
        .expect("Windows shell tests require Python 3 (stdlib only)");
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}
