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
    core::Capacity,
    packed::{Bytes, CellOutput},
    prelude::*,
};

use super::{
    chain::{normalize_tx_hash, parse_private_key},
    config::DeployConfig,
};

const SHANNONS_PER_CKB: u64 = 100_000_000;

#[derive(Clone, Debug)]
pub struct FundReceipt {
    pub tx_hash: String,
    pub recipient: String,
    pub amount_ckb: u64,
}

pub fn fund_address(
    config: &DeployConfig,
    recipient_address: &str,
    amount_ckb: u64,
) -> Result<FundReceipt> {
    let network_info = NetworkInfo::new(NetworkType::Testnet, config.rpc_url.clone());
    let configuration = TransactionBuilderConfiguration::new_with_network(network_info.clone())?;
    let deployer = Address::from_str(&config.deployer_address)
        .map_err(|err| anyhow::anyhow!("CKB_DEPLOYER_ADDRESS is not valid: {err}"))?;
    let recipient = Address::from_str(recipient_address)
        .map_err(|err| anyhow::anyhow!("recipient address is not valid: {err}"))?;

    let capacity = amount_ckb
        .checked_mul(SHANNONS_PER_CKB)
        .map(Capacity::shannons)
        .context("amount is too large")?;
    let output = CellOutput::new_builder()
        .lock(&recipient)
        .capacity(capacity.pack())
        .build();
    let min_capacity = output.occupied_capacity(Capacity::zero())?;
    if capacity < min_capacity {
        anyhow::bail!(
            "amount is below the recipient cell minimum: need at least {} CKB",
            min_capacity.as_u64() / SHANNONS_PER_CKB
        );
    }

    let iterator = InputIterator::new_with_address(&[deployer], &network_info);
    let mut builder = SimpleTransactionBuilder::new(configuration, iterator);
    builder.add_output_and_data(output, Bytes::default());

    let mut tx_with_groups = builder.build(&Default::default())?;
    let private_key = parse_private_key(&config.private_key)?;
    TransactionSigner::new(&network_info).sign_transaction(
        &mut tx_with_groups,
        &SignContexts::new_sighash_h256(vec![private_key])?,
    )?;

    let rpc = CkbRpcClient::new(&config.rpc_url);
    let json_tx = ckb_jsonrpc_types::TransactionView::from(tx_with_groups.get_tx_view().clone());
    let tx_hash = normalize_tx_hash(
        &rpc.send_transaction(json_tx.inner, None)
            .context("CKB send_transaction failed")?
            .to_string(),
    );

    Ok(FundReceipt {
        tx_hash,
        recipient: recipient_address.to_string(),
        amount_ckb,
    })
}
