use std::{path::PathBuf, str::FromStr};

use super::{
    chain::{CodeOutPoint, normalize_tx_hash, parse_private_key},
    config::{DeployConfig, DeployedScripts},
    vault_record::{write_env, write_record},
};
use anyhow::{Context, Result, anyhow};
use ckb_sdk::{
    Address, AddressPayload, CkbRpcClient, NetworkInfo, NetworkType,
    transaction::{
        TransactionBuilderConfiguration,
        builder::{CkbTransactionBuilder, SimpleTransactionBuilder},
        input::InputIterator,
        signer::{SignContexts, TransactionSigner},
    },
};
use ckb_types::{
    H256,
    core::{Capacity, DepType, ScriptHashType},
    packed::{Bytes, CellDep, CellOutput, OutPoint, Script},
    prelude::*,
};

const VAULT_DATA_LEN: usize = 33;
const EXTRA_FEE_MARGIN_SHANNONS: u64 = 1_000_000;

#[derive(Debug)]
pub struct VaultInitReceipt {
    pub tx_hash: String,
    pub vault_address: String,
    pub vault_out_point: CodeOutPoint,
    pub record_path: PathBuf,
}

pub fn init_vault(config: &DeployConfig) -> Result<VaultInitReceipt> {
    let scripts = RequiredScripts::load(&config.scripts)?;
    let network_info = NetworkInfo::new(NetworkType::Testnet, config.rpc_url.clone());
    let deployer = Address::from_str(&config.deployer_address)
        .map_err(|err| anyhow!("CKB_DEPLOYER_ADDRESS is not valid: {err}"))?;
    let admin_lock: Script = (&deployer).into();
    let built = build_vault_scripts(&scripts, &admin_lock)?;
    let data = empty_vault_data();
    let (output, output_data) = vault_output(&built.vault_lock, &built.vault_type, &data)?;

    let configuration = TransactionBuilderConfiguration::new_with_network(network_info.clone())?;
    let iterator = InputIterator::new_with_address(&[deployer], &network_info);
    let mut builder = SimpleTransactionBuilder::new(configuration, iterator);
    builder.add_output_and_data(output, output_data);

    let mut tx_with_groups = builder.build(&Default::default())?;
    add_vault_cell_deps(&mut tx_with_groups, &scripts)?;
    apply_fee_margin(&mut tx_with_groups)?;
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
    let vault_out_point = CodeOutPoint {
        tx_hash: tx_hash.clone(),
        index: "0x0".to_string(),
    };
    let vault_address = Address::new(
        NetworkType::Testnet,
        AddressPayload::from(built.vault_lock.clone()),
        true,
    )
    .to_string();

    let record_path = write_record(
        config,
        &tx_hash,
        &vault_address,
        &vault_out_point,
        &built.vault_lock_script_hash,
        &built.vault_type_script_hash,
    )?;
    write_env(&vault_address, &vault_out_point)?;

    Ok(VaultInitReceipt {
        tx_hash,
        vault_address,
        vault_out_point,
        record_path,
    })
}

struct RequiredScripts {
    vault_lock_code_hash: String,
    vault_lock_out_point: String,
    vault_type_code_hash: String,
    vault_type_out_point: String,
    lp_receipt_type_code_hash: String,
    request_type_code_hash: String,
    fee_claim_type_code_hash: String,
}

struct BuiltVaultScripts {
    vault_lock: Script,
    vault_type: Script,
    vault_lock_script_hash: String,
    vault_type_script_hash: String,
}

impl RequiredScripts {
    fn load(scripts: &DeployedScripts) -> Result<Self> {
        Ok(Self {
            vault_lock_code_hash: require(
                &scripts.vault_lock_code_hash,
                "LIQUIDLANE_VAULT_LOCK_CODE_HASH",
            )?,
            vault_lock_out_point: require(
                &scripts.vault_lock_out_point,
                "LIQUIDLANE_VAULT_LOCK_OUT_POINT",
            )?,
            vault_type_code_hash: require(
                &scripts.vault_type_code_hash,
                "LIQUIDLANE_VAULT_TYPE_CODE_HASH",
            )?,
            vault_type_out_point: require(
                &scripts.vault_type_out_point,
                "LIQUIDLANE_VAULT_TYPE_OUT_POINT",
            )?,
            lp_receipt_type_code_hash: require(
                &scripts.lp_receipt_type_code_hash,
                "LIQUIDLANE_LP_RECEIPT_TYPE_CODE_HASH",
            )?,
            request_type_code_hash: require(
                &scripts.request_type_code_hash,
                "LIQUIDLANE_REQUEST_TYPE_CODE_HASH",
            )?,
            fee_claim_type_code_hash: require(
                &scripts.fee_claim_type_code_hash,
                "LIQUIDLANE_FEE_CLAIM_TYPE_CODE_HASH",
            )?,
        })
    }
}

fn build_vault_scripts(
    scripts: &RequiredScripts,
    admin_lock: &Script,
) -> Result<BuiltVaultScripts> {
    let admin_lock_hash = admin_lock.calc_script_hash();
    let mut vault_type_args = Vec::with_capacity(128);
    vault_type_args.extend_from_slice(admin_lock_hash.as_slice());
    vault_type_args.extend_from_slice(hash_bytes(&scripts.lp_receipt_type_code_hash)?.as_bytes());
    vault_type_args.extend_from_slice(hash_bytes(&scripts.request_type_code_hash)?.as_bytes());
    vault_type_args.extend_from_slice(hash_bytes(&scripts.fee_claim_type_code_hash)?.as_bytes());
    let vault_type = data1_script(&scripts.vault_type_code_hash, vault_type_args)?;
    let vault_type_hash = vault_type.calc_script_hash();

    let mut vault_lock_args = Vec::with_capacity(160);
    vault_lock_args.extend_from_slice(admin_lock_hash.as_slice());
    vault_lock_args.extend_from_slice(vault_type_hash.as_slice());
    vault_lock_args.extend_from_slice(hash_bytes(&scripts.lp_receipt_type_code_hash)?.as_bytes());
    vault_lock_args.extend_from_slice(hash_bytes(&scripts.request_type_code_hash)?.as_bytes());
    vault_lock_args.extend_from_slice(hash_bytes(&scripts.fee_claim_type_code_hash)?.as_bytes());
    let vault_lock = data1_script(&scripts.vault_lock_code_hash, vault_lock_args)?;

    Ok(BuiltVaultScripts {
        vault_lock_script_hash: hex(vault_lock.calc_script_hash().as_slice()),
        vault_type_script_hash: hex(vault_type_hash.as_slice()),
        vault_lock,
        vault_type,
    })
}

fn vault_output(lock: &Script, type_script: &Script, data: &[u8]) -> Result<(CellOutput, Bytes)> {
    let output = CellOutput::new_builder()
        .lock(lock.clone())
        .type_(Some(type_script.clone()).pack())
        .build();
    let capacity = output
        .occupied_capacity(Capacity::bytes(data.len())?)?
        .pack();
    Ok((
        output.as_builder().capacity(capacity).build(),
        data.to_vec().pack(),
    ))
}

fn add_vault_cell_deps(
    tx: &mut ckb_sdk::TransactionWithScriptGroups,
    scripts: &RequiredScripts,
) -> Result<()> {
    let tx_view = tx
        .get_tx_view()
        .clone()
        .as_advanced_builder()
        .cell_dep(code_dep(&scripts.vault_lock_out_point)?)
        .cell_dep(code_dep(&scripts.vault_type_out_point)?)
        .build();
    tx.set_tx_view(tx_view);
    Ok(())
}

fn apply_fee_margin(tx: &mut ckb_sdk::TransactionWithScriptGroups) -> Result<()> {
    let mut outputs = tx.get_tx_view().outputs().into_iter().collect::<Vec<_>>();
    let Some(change) = outputs.last_mut() else {
        return Err(anyhow!("vault init transaction has no change output"));
    };
    let capacity: u64 = change.capacity().unpack();
    if capacity <= EXTRA_FEE_MARGIN_SHANNONS {
        return Err(anyhow!("change output cannot cover vault init fee margin"));
    }
    *change = change
        .clone()
        .as_builder()
        .capacity(capacity - EXTRA_FEE_MARGIN_SHANNONS)
        .build();
    let tx_view = tx
        .get_tx_view()
        .clone()
        .as_advanced_builder()
        .set_outputs(outputs)
        .build();
    tx.set_tx_view(tx_view);
    Ok(())
}

fn code_dep(out_point: &str) -> Result<CellDep> {
    Ok(CellDep::new_builder()
        .out_point(parse_out_point(out_point)?)
        .dep_type(DepType::Code)
        .build())
}

fn parse_out_point(value: &str) -> Result<OutPoint> {
    let (tx_hash, index) = value
        .split_once('#')
        .ok_or_else(|| anyhow!("out-point must be tx_hash#index"))?;
    Ok(OutPoint::new_builder()
        .tx_hash(parse_h256(tx_hash)?.pack())
        .index(u32::from_str_radix(index.trim_start_matches("0x"), 16)?)
        .build())
}

fn data1_script(code_hash: &str, args: Vec<u8>) -> Result<Script> {
    Ok(Script::new_builder()
        .code_hash(parse_h256(code_hash)?.pack())
        .hash_type(ScriptHashType::Data1)
        .args(args.pack())
        .build())
}

fn empty_vault_data() -> [u8; VAULT_DATA_LEN] {
    let mut data = [0u8; VAULT_DATA_LEN];
    data[0] = 1;
    data
}

fn require(value: &Option<String>, key: &str) -> Result<String> {
    value
        .clone()
        .ok_or_else(|| anyhow!("{key} is missing from .env"))
}

fn hash_bytes(value: &str) -> Result<H256> {
    parse_h256(value)
}

fn parse_h256(value: &str) -> Result<H256> {
    H256::from_str(value.trim_start_matches("0x")).context("expected 32-byte hex hash")
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(2 + bytes.len() * 2);
    out.push_str("0x");
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}
