use super::*;
#[test]
fn managed_rpc_stays_private_and_public_peers_cannot_trigger_provider_spending() {
    let template = include_str!("testnet.yml");
    let config = configuration(
        template,
        "https://testnet.ckb.dev/rpc",
        19027,
        19028,
        "provider",
        Ipv4Addr::LOCALHOST,
    )
    .unwrap();
    assert!(config.contains("listening_addr: \"127.0.0.1:19027\""));
    assert!(
        config.contains("open_channel_auto_accept_min_ckb_funding_amount: 18446744073709551615")
    );
    assert!(config.contains("auto_accept_channel_ckb_funding_amount: 0"));
    assert!(config.contains("- \"/ip4/127.0.0.1/tcp/19028\""));
    assert!(
        configuration(
            template,
            "file:///secret",
            19027,
            19028,
            "provider",
            Ipv4Addr::LOCALHOST
        )
        .is_err()
    );
    assert!(
        configuration(
            template,
            "https://testnet.ckb.dev/rpc",
            19027,
            19027,
            "provider",
            Ipv4Addr::LOCALHOST
        )
        .is_err()
    );
    assert!(
        configuration(
            "different template",
            "https://testnet.ckb.dev/rpc",
            19027,
            19028,
            "provider",
            Ipv4Addr::LOCALHOST
        )
        .is_err()
    );
}
#[test]
fn creating_a_key_never_replaces_existing_bytes() {
    let directory =
        std::env::temp_dir().join(format!("liquidlane-key-test-{}", uuid::Uuid::new_v4()));
    service::private_dir(&directory).unwrap();
    let path = directory.join("key");
    create_private(&path, b"original").unwrap();
    assert!(create_private(&path, b"replacement").is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"original");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
#[ignore = "requires LIQUIDLANE_TEST_FIBER_BINARY pointing to the real Fiber 0.9.0 binary"]
fn generated_configuration_is_accepted_by_native_fiber() {
    let binary = std::env::var_os("LIQUIDLANE_TEST_FIBER_BINARY").unwrap();
    let directory =
        std::env::temp_dir().join(format!("liquidlane-native-config-{}", uuid::Uuid::new_v4()));
    service::private_dir(&directory).unwrap();
    let config = configuration(
        include_str!("testnet.yml"),
        "https://testnet.ckbapp.dev/",
        19027,
        19028,
        "provider",
        Ipv4Addr::LOCALHOST,
    )
    .unwrap();
    let path = directory.join("config.yml");
    std::fs::write(&path, config).unwrap();
    // Native database validation parses the full configuration before checking
    // for a store. It exits before wallets, RPC listeners or network actors start.
    let output = std::process::Command::new(binary)
        .env_clear()
        .args(["--check-validate", "-d"])
        .arg(&directory)
        .arg("-c")
        .arg(&path)
        .output()
        .unwrap();
    let result = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        result.contains("store path does not exist:"),
        "native parsing did not reach store validation: {result}"
    );
    assert!(!directory.join("ckb").exists());
    assert!(!directory.join("fiber").exists());
    std::fs::remove_dir_all(directory).unwrap();
}
