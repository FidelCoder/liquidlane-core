use std::str::FromStr;

use anyhow::{Context, Result};
use ckb_sdk::{
    Address, CkbRpcClient, NetworkInfo, NetworkType,
    transaction::{
        TransactionBuilderConfiguration,
        builder::{CkbTransactionBuilder, SimpleTransactionBuilder},
        input::InputIterator,
        signer::{SignContexts, TransactionSigner},
    },
};
use ckb_types::{
    H256,
    core::Capacity,
    packed::{Bytes, CellOutput},
    prelude::*,
};

use super::{config::DeployConfig, scripts::ScriptArtifact};

#[derive(Clone, Debug)]
pub struct DeployReceipt {
    pub tx_hash: String,
    pub scripts: Vec<DeployedScript>,
}

#[derive(Clone, Debug)]
pub struct DeployedScript {
    pub name: String,
    pub out_point: CodeOutPoint,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct CodeOutPoint {
    pub tx_hash: String,
    pub index: String,
}

pub fn deploy_scripts(config: &DeployConfig, scripts: &[ScriptArtifact]) -> Result<DeployReceipt> {
    let network_info = NetworkInfo::new(NetworkType::Testnet, config.rpc_url.clone());
    let configuration = TransactionBuilderConfiguration::new_with_network(network_info.clone())?;
    let deployer = Address::from_str(&config.deployer_address)
        .map_err(|err| anyhow::anyhow!("CKB_DEPLOYER_ADDRESS is not valid: {err}"))?;

    let iterator = InputIterator::new_with_address(&[deployer.clone()], &network_info);
    let mut builder = SimpleTransactionBuilder::new(configuration, iterator);
    for script in scripts {
        let (output, data) = code_cell(&deployer, &script.data)?;
        builder.add_output_and_data(output, data);
    }

    let mut tx_with_groups = builder.build(&Default::default())?;
    let private_key = parse_private_key(&config.private_key)?;
    TransactionSigner::new(&network_info).sign_transaction(
        &mut tx_with_groups,
        &SignContexts::new_sighash_h256(vec![private_key])?,
    )?;

    let json_tx = ckb_jsonrpc_types::TransactionView::from(tx_with_groups.get_tx_view().clone());
    let tx_hash = normalize_tx_hash(
        &CkbRpcClient::new(&config.rpc_url)
            .send_transaction(json_tx.inner, None)
            .context("CKB send_transaction failed")?
            .to_string(),
    );

    let scripts = scripts
        .iter()
        .enumerate()
        .map(|(index, script)| DeployedScript {
            name: script.name.clone(),
            out_point: CodeOutPoint {
                tx_hash: tx_hash.clone(),
                index: format!("0x{index:x}"),
            },
        })
        .collect();

    Ok(DeployReceipt { tx_hash, scripts })
}

fn code_cell(deployer: &Address, data: &[u8]) -> Result<(CellOutput, Bytes)> {
    let data_capacity = Capacity::bytes(data.len())?;
    let dummy_output = CellOutput::new_builder().lock(deployer).build();
    let required_capacity = dummy_output.occupied_capacity(data_capacity)?.pack();
    let output = dummy_output
        .as_builder()
        .capacity(required_capacity)
        .build();
    Ok((output, data.to_vec().pack()))
}

fn parse_private_key(value: &str) -> Result<H256> {
    let value = value.trim().trim_start_matches("0x");
    Ok(H256::from_str(value).context("CKB_DEPLOYER_PRIVATE_KEY must be 32 bytes hex")?)
}

fn normalize_tx_hash(value: &str) -> String {
    if value.starts_with("0x") {
        value.to_string()
    } else {
        format!("0x{value}")
    }
}
