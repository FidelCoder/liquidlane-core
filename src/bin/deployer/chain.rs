use std::str::FromStr;

use anyhow::{Context, Result};
use ckb_sdk::{
    Address, CkbRpcClient, NetworkInfo, NetworkType,
    transaction::{
        TransactionBuilderConfiguration,
        builder::{CkbTransactionBuilder, SimpleTransactionBuilder},
        input::{InputIterator, TransactionInput},
        signer::{SignContexts, TransactionSigner},
    },
};
use ckb_types::{
    H256,
    bytes::Bytes as RawBytes,
    core::Capacity,
    packed::{Bytes, CellOutput, OutPoint},
    prelude::*,
};

use ckb_sdk::traits::LiveCell;

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

    let mut iterator = InputIterator::new_with_address(&[deployer.clone()], &network_info);
    let rpc = CkbRpcClient::new(&config.rpc_url);
    if config.spend_previous_script_cells {
        push_previous_script_inputs(&mut iterator, config, &rpc)?;
    }
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
        &rpc.send_transaction(json_tx.inner, None)
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

fn push_previous_script_inputs(
    iterator: &mut InputIterator,
    config: &DeployConfig,
    rpc: &CkbRpcClient,
) -> Result<()> {
    let mut inputs = Vec::new();
    for out_point in previous_script_out_points(config) {
        inputs.push(load_live_input(rpc, &out_point)?);
    }
    for input in inputs.into_iter().rev() {
        iterator.push_input(input);
    }
    Ok(())
}

fn previous_script_out_points(config: &DeployConfig) -> Vec<String> {
    [
        config.scripts.vault_lock_out_point.as_ref(),
        config.scripts.vault_type_out_point.as_ref(),
        config.scripts.lp_receipt_type_out_point.as_ref(),
        config.scripts.request_type_out_point.as_ref(),
        config.scripts.fee_claim_type_out_point.as_ref(),
    ]
    .into_iter()
    .flatten()
    .cloned()
    .collect()
}

fn load_live_input(rpc: &CkbRpcClient, value: &str) -> Result<TransactionInput> {
    let out_point = parse_out_point(value)?;
    let cell = rpc
        .get_live_cell(out_point.clone().into(), true)
        .context("CKB get_live_cell failed for previous script cell")?;
    if cell.status != "live" {
        anyhow::bail!("previous script cell {value} is not live: {}", cell.status);
    }
    let cell = cell
        .cell
        .ok_or_else(|| anyhow::anyhow!("previous script cell {value} has no cell data"))?;
    let output_data: RawBytes = cell
        .data
        .ok_or_else(|| anyhow::anyhow!("previous script cell {value} was fetched without data"))?
        .content
        .into_bytes();
    Ok(TransactionInput::new(
        LiveCell {
            output: cell.output.into(),
            output_data,
            out_point,
            block_number: 0,
            tx_index: 0,
        },
        0,
    ))
}

fn parse_out_point(value: &str) -> Result<OutPoint> {
    let (tx_hash, index) = value
        .split_once('#')
        .ok_or_else(|| anyhow::anyhow!("out-point must be tx_hash#index"))?;
    Ok(OutPoint::new_builder()
        .tx_hash(parse_h256(tx_hash)?.pack())
        .index(u32::from_str_radix(index.trim_start_matches("0x"), 16)?)
        .build())
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

fn parse_h256(value: &str) -> Result<H256> {
    let value = value.trim().trim_start_matches("0x");
    Ok(H256::from_str(value).context("expected 32-byte hex hash")?)
}

pub(crate) fn parse_private_key(value: &str) -> Result<H256> {
    let value = value.trim().trim_start_matches("0x");
    Ok(H256::from_str(value).context("CKB_DEPLOYER_PRIVATE_KEY must be 32 bytes hex")?)
}

pub(crate) fn normalize_tx_hash(value: &str) -> String {
    if value.starts_with("0x") {
        value.to_string()
    } else {
        format!("0x{value}")
    }
}
