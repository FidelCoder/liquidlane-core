//! Local Linux user services. No shell, root service, or remote RPC listener.
use super::config::{Config, write_private};
use crate::marketplace::crypto;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Serialize, Deserialize)]
pub struct Installed {
    pub unit: String,
    pub config: PathBuf,
    pub starts_at_boot: bool,
}

pub fn available() -> Result<()> {
    ensure!(
        cfg!(target_os = "linux"),
        "automatic background setup currently requires Linux with systemd; use normal setup on other systems"
    );
    control(&["show-environment"]).context(
        "no local user service manager; log in to your Linux user session before automatic setup",
    )?;
    Ok(())
}
pub fn private_dir(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}
pub fn data_root() -> Result<PathBuf> {
    let root = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or(
            PathBuf::from(std::env::var_os("HOME").context("home directory unavailable")?)
                .join(".local/share"),
        );
    ensure!(root.is_absolute(), "XDG_DATA_HOME must be absolute");
    let root = root.join("liquidlane");
    private_dir(&root)?;
    Ok(root)
}
pub fn quote(value: &str) -> Result<String> {
    Ok(literal(value)?.replace('$', "$$"))
}
fn literal(value: &str) -> Result<String> {
    ensure!(
        !value.chars().any(char::is_control),
        "service arguments cannot contain control characters"
    );
    Ok(format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('%', "%%")
    ))
}
pub fn path_argument(path: &Path) -> Result<String> {
    ensure!(path.is_absolute(), "service paths must be absolute");
    // The executable name does not expand environment variables.
    literal(path.to_str().context("service path must be UTF-8")?)
}
fn working_directory(path: &Path) -> Result<String> {
    ensure!(path.is_absolute(), "service paths must be absolute");
    let value = path.to_str().context("service path must be UTF-8")?;
    ensure!(
        value.trim() == value && !value.chars().any(char::is_control) && !value.contains('\\'),
        "service directory cannot contain control characters, backslashes or trailing whitespace"
    );
    // WorkingDirectory takes a raw path, not a quoted ExecStart argument.
    Ok(value.replace('%', "%%"))
}
pub fn unit_name(kind: &str, identity: &Path) -> Result<String> {
    ensure!(
        matches!(kind, "connector" | "fiber"),
        "invalid service kind"
    );
    let hash = crypto::digest(identity.to_str().context("path must be UTF-8")?.as_bytes());
    Ok(format!("liquidlane-{kind}-{}.service", &hash[..24]))
}
pub fn definition(
    description: &str,
    executable: &Path,
    args: &[&str],
    directory: &Path,
) -> Result<String> {
    ensure!(
        !description.chars().any(char::is_control),
        "invalid service description"
    );
    let mut command = vec![path_argument(executable)?];
    for argument in args {
        command.push(quote(argument)?);
    }
    Ok(format!(
        "# Managed by LiquidLane. Keys and funding policy stay local.\n[Unit]\nDescription={description}\nStartLimitIntervalSec=0\n\n[Service]\nType=exec\nWorkingDirectory={}\nExecStart={}\nRestart=on-failure\nRestartSec=5\nTimeoutStopSec=45\nKillSignal=SIGINT\nUMask=0077\nNoNewPrivileges=yes\nRestrictSUIDSGID=yes\nLockPersonality=yes\nRestrictAddressFamilies=AF_UNIX AF_INET AF_INET6\nLimitCORE=0\nEnvironment=RUST_LOG=info\n\n[Install]\nWantedBy=default.target\n",
        working_directory(directory)?,
        command.join(" ")
    ))
}
pub fn install_unit(unit: &str, body: &str) -> Result<()> {
    validate_name(unit)?;
    let root = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or(
            PathBuf::from(std::env::var_os("HOME").context("home directory unavailable")?)
                .join(".config"),
        );
    ensure!(root.is_absolute(), "XDG_CONFIG_HOME must be absolute");
    let directory = root.join("systemd/user");
    std::fs::create_dir_all(&directory)?;
    let path = directory.join(unit);
    if path.exists() {
        ensure!(
            std::fs::read_to_string(&path)?.starts_with("# Managed by LiquidLane."),
            "refusing to overwrite an unrelated service"
        );
    }
    // write_private is JSON-specific; unit bytes are written atomically with mode 0600.
    let temporary = directory.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    use std::io::Write;
    let mut file = options.open(&temporary)?;
    file.write_all(body.as_bytes())?;
    file.sync_all()?;
    std::fs::rename(temporary, path)?;
    control(&["daemon-reload"])?;
    control(&["enable", "--now", unit])?;
    Ok(())
}
pub fn control(args: &[&str]) -> Result<String> {
    let output = Command::new("systemctl")
        .args(["--user", "--no-ask-password"])
        .args(args)
        .output()?;
    ensure!(
        output.status.success(),
        "user service action failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
fn validate_name(unit: &str) -> Result<()> {
    ensure!(
        unit.starts_with("liquidlane-")
            && unit.ends_with(".service")
            && unit
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.')),
        "invalid LiquidLane service name"
    );
    Ok(())
}
pub fn enable_boot() -> Result<bool> {
    let user = Command::new("id").arg("-un").output()?;
    ensure!(user.status.success(), "cannot identify local user");
    let user = String::from_utf8(user.stdout)?.trim().to_owned();
    let output = Command::new("loginctl")
        .args(["--no-ask-password", "enable-linger", &user])
        .output()?;
    if !output.status.success() {
        println!(
            "Automatic start at login is enabled. This system requires its administrator to enable start before login: loginctl enable-linger {user}"
        );
        return Ok(false);
    }
    let check = Command::new("loginctl")
        .args(["show-user", &user, "-p", "Linger", "--value"])
        .output()?;
    Ok(check.status.success() && String::from_utf8_lossy(&check.stdout).trim() == "yes")
}
pub fn install(config_path: &Path) -> Result<Installed> {
    available()?;
    let path = config_path.canonicalize()?;
    let config = Config::load(&path)?;
    let connector = super::Connector::load(config.clone())?;
    ensure!(
        connector.registration.expires_at > crate::marketplace::model::now(),
        "pair this node again before enabling automatic startup"
    );
    let unit = unit_name("connector", &config.state_dir.canonicalize()?)?;
    drop(preflight(&config)?);
    private_dir(&config.state_dir)?;
    let executable = std::env::current_exe()?.canonicalize()?;
    let body = definition(
        "LiquidLane node connector",
        &executable,
        &["run", path.to_str().context("config path must be UTF-8")?],
        path.parent().context("config folder missing")?,
    )?;
    install_unit(&unit, &body)?;
    control(&["restart", &unit])?;
    let installed = Installed {
        unit,
        config: path,
        starts_at_boot: enable_boot()?,
    };
    write_private(&config.state_dir.join("service.json"), &installed)?;
    println!("Connector runs automatically in the background. You may close this terminal.");
    println!(
        "Starts at {} and restarts after a crash. Keys and local funding limits stay on this machine.",
        if installed.starts_at_boot {
            "boot"
        } else {
            "login"
        }
    );
    println!("Service: {}", installed.unit);
    println!(
        "Status: '{}' service-status '{}'",
        executable.to_string_lossy().replace('\'', "'\\''"),
        installed.config.to_string_lossy().replace('\'', "'\\''")
    );
    Ok(installed)
}
pub fn action(config_path: &Path, action: &str) -> Result<()> {
    let config = Config::load(config_path)?;
    let installed: Installed = serde_json::from_slice(
        &std::fs::read(config.state_dir.join("service.json"))
            .context("automatic startup is not installed for this connector")?,
    )?;
    validate_name(&installed.unit)?;
    ensure!(
        installed.unit == unit_name("connector", &config.state_dir.canonicalize()?)?
            && installed.config == config_path.canonicalize()?,
        "service metadata belongs to a different connector"
    );
    match action {
        "service-status" => print!(
            "{}",
            control(&[
                "show",
                &installed.unit,
                "--property=ActiveState,SubState,MainPID,NRestarts,UnitFileState"
            ])?
        ),
        "service-stop" => {
            control(&["disable", "--now", &installed.unit])?;
            println!(
                "Connector stopped; automatic startup disabled. Existing channel funds remain on Fiber."
            );
        }
        "service-start" => {
            control(&["enable", "--now", &installed.unit])?;
            println!("Connector automatic startup enabled.");
        }
        _ => anyhow::bail!("unknown service action"),
    }
    Ok(())
}

pub fn preflight(config: &Config) -> Result<Option<std::fs::File>> {
    let unit = unit_name("connector", &config.state_dir.canonicalize()?)?;
    if control(&["is-active", &unit]).is_ok() {
        return Ok(None);
    }
    // Hold this across setup so a manual worker cannot race token/config replacement.
    config.run_lock().map(Some)
}

#[cfg(test)]
#[path = "service_tests.rs"]
mod tests;
