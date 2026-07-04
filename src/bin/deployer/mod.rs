mod chain;
mod config;
mod record;
mod scripts;

use anyhow::Result;

pub fn run() -> Result<()> {
    let config = config::DeployConfig::load()?;
    let scripts = scripts::load_script_artifacts(&config.build_dir)?;
    println!(
        "Deploying {} LiquidLane scripts to {} from {}",
        scripts.len(),
        config.network,
        config.deployer_address
    );

    let receipt = chain::deploy_scripts(&config, &scripts)?;
    let record_path = record::write_record(&config, &scripts, &receipt)?;

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
