use std::process::Command;

#[test]
fn test_cli_version_flag() {
    let bin = env!("CARGO_BIN_EXE_isolock");
    let output = Command::new(bin)
        .arg("--version")
        .output()
        .expect("runs isolock --version");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), format!("isolock {}", isolock_core::VERSION));
}

#[test]
fn test_cli_default_usage_output() {
    let bin = env!("CARGO_BIN_EXE_isolock");
    let output = Command::new(bin)
        .output()
        .expect("runs isolock without arguments");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(&format!("isolock v{}", isolock_core::VERSION)));
    assert!(stdout.contains("dependency airlock manager"));
}

#[test]
fn test_cli_subcommands() {
    let bin = env!("CARGO_BIN_EXE_isolock");

    let plan_out = Command::new(bin)
        .args(["plan", "--ci"])
        .output()
        .expect("runs isolock plan --ci");
    assert!(plan_out.status.success());

    let serve_out = Command::new(bin)
        .arg("serve")
        .output()
        .expect("runs isolock serve");
    assert!(serve_out.status.success());

    let ingest_out = Command::new(bin)
        .arg("ingest")
        .output()
        .expect("runs isolock ingest");
    assert!(ingest_out.status.success());
}
