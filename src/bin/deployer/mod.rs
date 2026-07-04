mod chain;
mod config;
mod record;
mod scripts;
mod vault;
mod vault_record;

use anyhow::Result;

pub fn run() -> Result<()> {
    let config = config::DeployConfig::load()?;
    match std::env::args().nth(1).as_deref() {
        Some("init-vault") => init_vault(&config),
        Some("deploy-scripts") | Some("deploy") | None => deploy_scripts(&config),
        Some(command) => anyhow::bail!("unknown deployer command: {command}"),
    }
}

fn deploy_scripts(config: &config::DeployConfig) -> Result<()> {
    let scripts = scripts::load_script_artifacts(&config.build_dir)?;
    println!(
        "Deploying {} LiquidLane scripts to {} from {}",
        scripts.len(),
        config.network,
        config.deployer_address
    );

    let receipt = chain::deploy_scripts(config, &scripts)?;
    let record_path = record::write_record(config, &scripts, &receipt)?;
    record::write_env(&scripts, &receipt)?;

    println!("Deployment tx: {}", receipt.tx_hash);
    println!("Record written: {}", record_path.display());
    for script in &receipt.scripts {
        println!(
            "{} -> {}#{}",
            script.name, script.out_point.tx_hash, script.out_point.index
        );
    }

    Ok(())
}

fn init_vault(config: &config::DeployConfig) -> Result<()> {
    let receipt = vault::init_vault(config)?;
    println!("Vault init tx: {}", receipt.tx_hash);
    println!("Vault address: {}", receipt.vault_address);
    println!(
        "Vault out-point: {}#{}",
        receipt.vault_out_point.tx_hash, receipt.vault_out_point.index
    );
    println!("Record written: {}", receipt.record_path.display());
    Ok(())
}
