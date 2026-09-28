use super::*;
#[test]
fn arguments_cannot_inject_units_commands_or_environment() {
    assert!(quote("path\nExecStart=bad").is_err());
    assert!(quote("path\0secret").is_err());
    assert_eq!(quote("a %n $HOME").unwrap(), "\"a %%n $$HOME\"");
    assert_eq!(
        path_argument(Path::new("/owned/$HOME/%n")).unwrap(),
        "\"/owned/$HOME/%%n\""
    );
    assert!(path_argument(Path::new("relative")).is_err());
    assert!(validate_name("../other.service").is_err());
}
#[test]
fn service_uses_exact_paths_without_a_shell_and_limits_crash_recovery() {
    let body = definition(
        "test",
        Path::new("/owned/connector"),
        &["run", "/owned/my node/config.json"],
        Path::new("/owned/$data/%n/my node"),
    )
    .unwrap();
    assert!(body.contains("ExecStart=\"/owned/connector\" \"run\" \"/owned/my node/config.json\""));
    assert!(body.contains("Restart=on-failure\n"));
    assert!(body.contains("WorkingDirectory=/owned/$data/%%n/my node\n"));
    assert!(body.contains("UMask=0077\n"));
    assert!(body.contains("LimitCORE=0\n"));
    assert!(!body.contains("/bin/sh"));
    assert!(!body.contains("User=root"));
    assert!(!body.contains("PRIVATE_KEY"));
}

#[test]
#[ignore = "requires the systemd-analyze parser, but does not contact or start a service"]
fn generated_service_is_accepted_by_systemd() {
    let directory =
        std::env::temp_dir().join(format!("liquidlane-service-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("liquidlane-verify.service");
    let body = definition(
        "LiquidLane parser check",
        Path::new("/usr/bin/true"),
        &["run", "/owned/my $node/%folder/config.json"],
        Path::new("/tmp/my $node/%folder"),
    )
    .unwrap();
    std::fs::write(&path, body).unwrap();
    let output = std::process::Command::new("systemd-analyze")
        .args(["--man=no", "verify"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::remove_dir_all(directory).unwrap();
}
